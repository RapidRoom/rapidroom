import 'i18next';
import en from '../i18n/locales/en.json';
import rapidroomEn from '../i18n/locales/rapidroom/en.json';

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation';
    resources: {
      translation: typeof en & typeof rapidroomEn;
    };
  }
}
