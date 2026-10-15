import type { TFunction } from 'i18next';

export interface LightroomSettingReport {
  settings: { field: string; outcome: 'mapped' | 'approximated' | 'ignored'; reason: string }[];
  mapped: number;
  approximated: number;
  ignored: number;
  omitted: number;
}

export function describeLightroomSettings(report: LightroomSettingReport, t: TFunction): string {
  const prefix = 'contextMenus.xmpImportReport.settings';
  const summary = t(`${prefix}.summary`, { ...report });
  const details = report.settings.slice(0, 128).map(({ field, outcome, reason }) =>
    t(`${prefix}.detail`, {
      field: field.slice(0, 64),
      outcome: t(`${prefix}.outcomes.${outcome}`),
      reason: t(`${prefix}.reasons.${reason}`, { defaultValue: t(`${prefix}.reasons.unsupported`) }),
    }),
  );
  if (report.omitted > 0) details.push(t(`${prefix}.omitted`, { count: report.omitted }));
  return [summary, ...details].join('\n');
}
