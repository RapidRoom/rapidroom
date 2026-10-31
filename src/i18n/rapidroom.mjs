import en from './locales/rapidroom/en.json' with { type: 'json' };
import de from './locales/rapidroom/de.json' with { type: 'json' };
import zhCN from './locales/rapidroom/zh-CN.json' with { type: 'json' };
import zhTW from './locales/rapidroom/zh-TW.json' with { type: 'json' };
import pl from './locales/rapidroom/pl.json' with { type: 'json' };
import es from './locales/rapidroom/es.json' with { type: 'json' };
import fr from './locales/rapidroom/fr.json' with { type: 'json' };
import it from './locales/rapidroom/it.json' with { type: 'json' };
import pt from './locales/rapidroom/pt.json' with { type: 'json' };
import ja from './locales/rapidroom/ja.json' with { type: 'json' };
import ko from './locales/rapidroom/ko.json' with { type: 'json' };
import ru from './locales/rapidroom/ru.json' with { type: 'json' };
import uk from './locales/rapidroom/uk.json' with { type: 'json' };
import ca from './locales/rapidroom/ca.json' with { type: 'json' };
import nl from './locales/rapidroom/nl.json' with { type: 'json' };
import cs from './locales/rapidroom/cs.json' with { type: 'json' };

const overlays = { en, de, 'zh-CN': zhCN, 'zh-TW': zhTW, pl, es, fr, it, pt, ja, ko, ru, uk, ca, nl, cs };
const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);

// Objects merge recursively; leaves (including arrays) belong to the overlay.
export function deepMerge(upstream, overlay) {
  if (!isObject(upstream) || !isObject(overlay)) return structuredClone(overlay);
  const merged = structuredClone(upstream);
  for (const [key, value] of Object.entries(overlay)) {
    merged[key] = deepMerge(upstream[key], value);
  }
  return merged;
}

export function withRapidRoomLocales(resources) {
  return Object.fromEntries(
    Object.entries(resources).map(([locale, resource]) => [
      locale,
      { ...resource, translation: deepMerge(resource.translation, overlays[locale] ?? {}) },
    ]),
  );
}
