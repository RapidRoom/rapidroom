import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import prettier from 'prettier';

const rootOption = process.argv.indexOf('--source-root');
const root =
  rootOption < 0
    ? path.dirname(path.dirname(fileURLToPath(import.meta.url)))
    : path.resolve(process.argv[rootOption + 1]);
const output = path.join(root, 'rapidroom/adjustment-schema.json');
const sourceName = 'src/utils/adjustments.ts';
const source = ts.createSourceFile(
  sourceName,
  fs.readFileSync(path.join(root, sourceName), 'utf8'),
  ts.ScriptTarget.Latest,
  true,
);
const constants = new Map();
const enums = new Map();
for (const statement of source.statements) {
  if (ts.isVariableStatement(statement)) {
    for (const declaration of statement.declarationList.declarations) {
      if (ts.isIdentifier(declaration.name)) constants.set(declaration.name.text, declaration.initializer);
    }
  }
  if (ts.isEnumDeclaration(statement)) {
    enums.set(
      statement.name.text,
      Object.fromEntries(statement.members.map((member) => [member.name.getText(source), member.initializer?.text])),
    );
  }
}

// Evaluate only literal source data, never import the app or execute its code.
function literal(node, depth = 0) {
  if (!node || depth > 32) throw new Error('Unsupported or recursive default expression');
  if (ts.isParenthesizedExpression(node) || ts.isAsExpression(node) || ts.isSatisfiesExpression(node))
    return literal(node.expression, depth + 1);
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return node.text;
  if (ts.isNumericLiteral(node)) return Number(node.text);
  if (node.kind === ts.SyntaxKind.TrueKeyword) return true;
  if (node.kind === ts.SyntaxKind.FalseKeyword) return false;
  if (node.kind === ts.SyntaxKind.NullKeyword) return null;
  if (ts.isPrefixUnaryExpression(node) && node.operator === ts.SyntaxKind.MinusToken)
    return -literal(node.operand, depth + 1);
  if (ts.isIdentifier(node) && constants.has(node.text)) return literal(constants.get(node.text), depth + 1);
  if (ts.isObjectLiteralExpression(node)) {
    const value = {};
    for (const property of node.properties) {
      if (ts.isSpreadAssignment(property)) Object.assign(value, literal(property.expression, depth + 1));
      else if (ts.isPropertyAssignment(property))
        value[property.name.text ?? property.name.getText(source)] = literal(property.initializer, depth + 1);
      else throw new Error('Unsupported default object member');
    }
    return value;
  }
  if (ts.isArrayLiteralExpression(node)) return node.elements.map((element) => literal(element, depth + 1));
  if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.arguments.length === 0) {
    const fn = constants.get(node.expression.text);
    if (fn && ts.isArrowFunction(fn) && !ts.isBlock(fn.body)) return literal(fn.body, depth + 1);
  }
  throw new Error('Unsupported default expression: ' + node.getText(source));
}

const defaults = literal(constants.get('INITIAL_ADJUSTMENTS'));
const parameters = {};
function flatten(value, prefix = '') {
  for (const [key, item] of Object.entries(value)) {
    const name = prefix ? prefix + '.' + key : key;
    parameters[name] = {
      defaultType: item === null ? 'null' : Array.isArray(item) ? 'array' : typeof item,
      default: item,
    };
    if (item && typeof item === 'object' && !Array.isArray(item)) flatten(item, name);
  }
}
flatten(defaults);

function files(folder) {
  return fs.readdirSync(folder, { withFileTypes: true }).flatMap((entry) => {
    const name = path.join(folder, entry.name);
    return entry.isDirectory() ? files(name) : name.endsWith('.tsx') ? [name] : [];
  });
}

const controls = [];
const sourceFiles = files(path.join(root, 'src/components/adjustments'))
  .concat([
    path.join(root, 'src/components/panel/right/CropPanel.tsx'),
    path.join(root, 'src/components/ui/ColorWheel.tsx'),
  ])
  .sort();
for (const file of sourceFiles) {
  const text = fs.readFileSync(file, 'utf8');
  const tree = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const visit = (node) => {
    if ((ts.isJsxSelfClosingElement(node) || ts.isJsxOpeningElement(node)) && node.tagName.getText(tree) === 'Slider') {
      const attributes = Object.fromEntries(
        node.attributes.properties
          .filter(ts.isJsxAttribute)
          .map((attribute) => [
            attribute.name.text,
            attribute.initializer && ts.isJsxExpression(attribute.initializer)
              ? attribute.initializer.expression
              : attribute.initializer,
          ]),
      );
      const number = (name) => {
        const value = attributes[name];
        try {
          return typeof literal(value) === 'number' ? literal(value) : null;
        } catch {
          return null;
        }
      };
      const expression = attributes.value?.getText(tree) ?? '';
      const onChange = attributes.onChange?.getText(tree) ?? '';
      const referenced = new Set();
      for (const [enumName, members] of enums) {
        for (const [member, key] of Object.entries(members)) {
          if ((expression + ' ' + onChange).includes(enumName + '.' + member)) {
            if (key in defaults) referenced.add(key);
            else if (enumName === 'ColorGrading' && 'colorGrading.' + key in parameters)
              referenced.add('colorGrading.' + key);
          }
        }
      }
      if (attributes['data-adjustment-key']) {
        const pattern = literal(attributes['data-adjustment-key']);
        const expression = new RegExp(
          '^' +
            pattern
              .split('*')
              .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
              .join('.*') +
            '$',
        );
        for (const key of Object.keys(parameters)) if (expression.test(key)) referenced.add(key);
        if (!referenced.size) throw new Error('Schema binding matched no key: ' + pattern);
      }
      const direct = /adjustments\.([A-Za-z][A-Za-z0-9]*)/.exec(expression);
      if (direct && direct[1] in defaults) referenced.add(direct[1]);
      if (/^hsl\[/.test(expression))
        for (const key of Object.keys(parameters)) if (/^hsl\.[^.]+\.[^.]+$/.test(key)) referenced.add(key);
      const control = {
        source: path.relative(root, file),
        line: tree.getLineAndCharacterOfPosition(node.getStart(tree)).line + 1,
        valueExpression: expression,
        changeExpression: onChange,
        labelExpression: attributes.label?.getText(tree) ?? '',
        minimum: number('min'),
        maximum: number('max'),
        step: number('step'),
        adjustmentKeys: [...referenced].sort(),
      };
      controls.push(control);
      for (const key of referenced) {
        const parameter = parameters[key];
        if (parameter.defaultType !== 'number') continue;
        const range = {
          minimum: control.minimum,
          maximum: control.maximum,
          step: control.step,
          source: control.source,
          valueExpression: control.valueExpression,
        };
        (parameter.uiRanges ??= []).push(range);
        parameter.signConvention =
          parameter.default === 0 && control.minimum < 0
            ? 'Zero is the neutral control value; positive values increase this UI control, negative values decrease it.'
            : 'Use the UI control value directly, within its listed range; do not normalize it to 0–1.';
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(tree);
}

const schema = {
  version: 1,
  generatedFrom: [sourceName, ...sourceFiles.map((file) => path.relative(root, file))],
  conventions: {
    values:
      'Native RapidRoom adjustment values, identical to the editor controls. Ranges below are UI limits; MCP validation may accept other finite values.',
    crop: 'Crop coordinates retain their explicit unit (% or px).',
    controls:
      'All slider props are recorded. Where a dynamic component binding is unresolved, consult its recorded value/change expression rather than inventing a range.',
  },
  parameters,
  controls,
};
const rendered = await prettier.format(JSON.stringify(schema), {
  ...(await prettier.resolveConfig(fileURLToPath(import.meta.url))),
  parser: 'json',
});
if (process.argv.includes('--check')) {
  if (!fs.existsSync(output) || fs.readFileSync(output, 'utf8') !== rendered)
    throw new Error('Adjustment schema drift: run node rapidroom/generate-adjustment-schema.mjs');
} else {
  fs.writeFileSync(output, rendered);
}
console.log(`Generated schema: ${Object.keys(parameters).length} keys, ${controls.length} source slider controls`);
