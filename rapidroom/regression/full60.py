#!/usr/bin/env python3
"""Strict full-corpus planning, shard validation and canonical reference artifacts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

SETS = ('neutral', 'busy-agx-lut-effects', 'busy-tone-color-detail')
SHARDS = 4


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def digest(path):
    with open(path, 'rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def corpus_ids(source):
    corpus = Path(source) / 'rapidroom/regression/corpus.json'
    files = read(corpus)['files']
    ids = [item['id'] for item in files]
    require(len(ids) == 20 and len(set(ids)) == 20, 'canonical corpus must contain 20 unique RAW IDs')
    require(all(re.fullmatch(r'[a-z0-9-]+', item) for item in ids), 'invalid corpus ID')
    return ids, digest(corpus)


def keys(ids):
    return {(setting, image) for setting in SETS for image in ids}


def image_rows(rows, expected, rendered=False):
    result = {}
    for row in rows:
        key = row['set'], row['id']
        require(key not in result, f'duplicate case: {key}')
        require(not rendered or row.get('ok') is True, f'render failed: {key}')
        require(isinstance(row.get('width'), int) and row['width'] > 0, f'invalid width: {key}')
        require(isinstance(row.get('height'), int) and row['height'] > 0, f'invalid height: {key}')
        require(bool(re.fullmatch(r'[0-9a-f]{64}', row.get('pixel_sha256', ''))), f'missing pixel hash: {key}')
        result[key] = {field: row[field] for field in ('set', 'id', 'width', 'height', 'pixel_sha256')}
    require(set(result) == expected, f'case coverage differs: missing={sorted(expected - set(result))}, extra={sorted(set(result) - expected)}')
    return result


def recipe(manifest):
    return {
        'corpus_sha256': manifest['corpus_sha256'],
        'adjustment_sets': {name: manifest['adjustment_sets'][name]['sha256'] for name in sorted(SETS)},
        'lut_sha256': manifest.get('lut_sha256'),
    }


def validate_reference(reference, ids, corpus_sha):
    require(reference.get('kind') == 'rapidraw-ci-reference', 'invalid full reference kind')
    require(bool(re.fullmatch(r'[0-9a-f]{40}', reference.get('source_sha', ''))), 'full reference lacks pinned source SHA')
    require(reference.get('corpus_sha256') == corpus_sha, 'full reference corpus differs')
    require(set(reference.get('adjustment_sets', {})) == set(SETS), 'full reference adjustment sets differ')
    return image_rows(reference['images'], keys(ids))


def resolve_source(source, requested, record):
    def git(*args):
        return subprocess.check_output(['git', '-C', str(source), *args], text=True).strip()
    main = git('rev-parse', 'origin/main^{commit}')
    if record:
        require(bool(re.fullmatch(r'[0-9a-f]{40}', requested)), 'record mode requires a pinned 40-character main SHA')
        require(subprocess.run(['git', '-C', str(source), 'merge-base', '--is-ancestor', requested, main]).returncode == 0, 'record source must be an ancestor of main')
        git('checkout', '--detach', requested)
        return requested
    require(not requested, 'source_sha override is only allowed when recording')
    return main


def plan(args):
    source_sha = resolve_source(args.source, args.source_sha, args.record)
    ids, corpus_sha = corpus_ids(args.source)
    if not args.record:
        require(Path(args.reference).is_file(), 'full reference missing')
        validate_reference(read(args.reference), ids, corpus_sha)
    matrix = [{'shard': i, 'only': ','.join(ids[i::SHARDS])} for i in range(SHARDS)]
    out = {'source_sha': source_sha, 'corpus_sha256': corpus_sha, 'ids': ids, 'matrix': {'include': matrix}}
    write(args.out, out)
    if args.github_output:
        with open(args.github_output, 'a') as output:
            output.write(f'source_sha={source_sha}\n')
            output.write('matrix=' + json.dumps(out['matrix'], separators=(',', ':')) + '\n')


def validate_shard(source, shard, first, second, engine):
    ids, corpus_sha = corpus_ids(source)
    require(shard in range(SHARDS), 'invalid shard number')
    expected = keys(ids[shard::SHARDS])
    a = image_rows(first['renders'], expected, rendered=True)
    b = image_rows(second['renders'], expected, rendered=True)
    changed = [key for key in sorted(a) if a[key] != b[key]]
    require(not changed, f'nondeterministic pixels or dimensions between shard passes: {changed}')
    require(recipe(first) == recipe(second), 'render recipe changed between passes')
    require(first['corpus_sha256'] == corpus_sha, 'rendered corpus differs from pinned source')
    require(first['engine']['sha256'] == second['engine']['sha256'] == engine['engine_sha256'], 'engine differs between build and render passes')
    return {
        'shard': shard, 'source_sha': engine['source_sha'], 'engine_sha256': engine['engine_sha256'],
        'recipe': recipe(first), 'images': [a[key] for key in sorted(a)], 'deterministic': True,
    }


def shard(args):
    report = Path(args.out)
    report.mkdir(parents=True, exist_ok=True)
    try:
        for source, name in ((args.first, 'run1-manifest.json'), (args.second, 'run2-manifest.json'), (args.engine, 'engine.json')):
            if Path(source).is_file():
                (report / name).write_bytes(Path(source).read_bytes())
        engine = read(args.engine)
        require(engine['source_sha'] == args.source_sha, 'engine source SHA differs from selected main')
        result = validate_shard(args.source, args.shard, read(args.first), read(args.second), engine)
        write(report / 'shard.json', result)
        write(report / 'provenance.json', {
            'engine': engine, 'first': read(args.first), 'second': read(args.second),
        })
    except Exception as error:
        write(report / 'shard.json', {'shard': args.shard, 'source_sha': args.source_sha, 'deterministic': False, 'error': str(error)})
        raise


def combine(source, source_sha, reports):
    ids, corpus_sha = corpus_ids(source)
    require(len(reports) == SHARDS and {row['shard'] for row in reports} == set(range(SHARDS)), 'missing or duplicate shard reports')
    require(all(row.get('deterministic') is True for row in reports), 'a shard failed or was nondeterministic')
    require(all(row['source_sha'] == source_sha for row in reports), 'mixed source SHAs')
    require(len({row['engine_sha256'] for row in reports}) == 1, 'shards did not share one engine')
    require(all(row['recipe'] == reports[0]['recipe'] for row in reports), 'mixed render recipes')
    require(reports[0]['recipe']['corpus_sha256'] == corpus_sha, 'shard corpus differs from source')
    for row in reports:
        image_rows(row['images'], keys(ids[row['shard']::SHARDS]))
    rows = image_rows([image for row in reports for image in row['images']], keys(ids))
    # Volatile timestamps, kernel/host details and binary hashes live in the
    # provenance artifact. The golden file pins the source, recipe and all pixels.
    return {
        '_comment': 'Full60 Mesa lavapipe pixel reference. Recorded from pinned main; two independent hosted recordings must agree byte-for-byte. Run/engine/environment provenance is retained separately. Never updated automatically.',
        'kind': 'rapidraw-ci-reference', 'source_sha': source_sha,
        'source': f'https://github.com/RapidRoom/rapidroom/commit/{source_sha}',
        'renderer': 'Mesa lavapipe (verified llvmpipe Vulkan)', **reports[0]['recipe'],
        'images': [rows[key] for key in sorted(rows)],
    }


def compare(reference, current):
    for field in ('corpus_sha256', 'adjustment_sets', 'lut_sha256'):
        require(reference[field] == current[field], f'full reference recipe differs: {field}')
    a = {(row['set'], row['id']): row for row in reference['images']}
    b = {(row['set'], row['id']): row for row in current['images']}
    require(set(a) == set(b), 'full reference coverage differs')
    return [{'set': key[0], 'id': key[1], 'reference': a[key], 'current': b[key]} for key in sorted(a) if a[key] != b[key]]


def collect(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    reports = [read(path) for path in sorted(Path(args.reports).glob('*/shard.json'))]
    current = combine(args.source, args.source_sha, reports)
    provenance = {
        'source_sha': args.source_sha, 'engine_sha256': reports[0]['engine_sha256'],
        'run_id': os.environ.get('GITHUB_RUN_ID'), 'workflow_sha': os.environ.get('GITHUB_SHA'),
        'shards': reports,
    }
    write(out / 'provenance.json', provenance)
    if args.record:
        write(out / 'ci-reference-full60.json', current)
        message = 'Recorded 60 deterministic cases from pinned main; candidate artifact only, no reference was committed.'
    else:
        require(Path(args.reference).is_file(), 'full reference missing')
        ids, corpus_sha = corpus_ids(args.source)
        reference = read(args.reference)
        validate_reference(reference, ids, corpus_sha)
        differences = compare(reference, current)
        write(out / 'comparison.json', {'n_images': 60, 'n_identical': 60 - len(differences), 'n_failed': len(differences), 'differences': differences, 'pass': not differences})
        require(not differences, f'full60 pixels/dimensions differ in {len(differences)} cases; see comparison.json')
        message = 'All 60 cases are deterministic and match the committed full reference exactly.'
    print(message)
    if args.summary:
        with open(args.summary, 'a') as summary:
            summary.write(f'## Full60 regression\n\n{message}\n\nSource: `{args.source_sha}`\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    p = commands.add_parser('plan')
    p.add_argument('--source', required=True)
    p.add_argument('--source-sha', default='')
    p.add_argument('--reference', required=True)
    p.add_argument('--record', action='store_true')
    p.add_argument('--out', required=True)
    p.add_argument('--github-output')
    p.add_argument('--summary')
    p.set_defaults(function=plan)
    p = commands.add_parser('shard')
    p.add_argument('--source', required=True)
    p.add_argument('--source-sha', required=True)
    p.add_argument('--shard', type=int, required=True)
    p.add_argument('--first', required=True)
    p.add_argument('--second', required=True)
    p.add_argument('--engine', required=True)
    p.add_argument('--out', required=True)
    p.set_defaults(function=shard)
    p = commands.add_parser('collect')
    p.add_argument('--source', required=True)
    p.add_argument('--source-sha', required=True)
    p.add_argument('--reports', required=True)
    p.add_argument('--reference', required=True)
    p.add_argument('--record', action='store_true')
    p.add_argument('--out', required=True)
    p.add_argument('--summary')
    p.set_defaults(function=collect)
    args = parser.parse_args()
    try:
        args.function(args)
    except (ValueError, KeyError, OSError) as error:
        print(f'::error::Full60 regression: {error}', file=sys.stderr)
        if getattr(args, 'summary', None):
            with open(args.summary, 'a') as summary:
                summary.write(f'## Full60 regression FAILED\n\n{error}\n')
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
