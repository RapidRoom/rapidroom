import { describe, expect, it } from 'vitest';
import { Theme } from '../components/ui/AppProperties';
import { THEMES } from './themes';

describe('themes', () => {
  it('offers Classic without changing the existing theme definitions', () => {
    expect(THEMES.map((theme) => theme.id)).toEqual([Theme.Dark, Theme.Light, Theme.Grey, Theme.Classic]);
    expect(THEMES.find((theme) => theme.id === Theme.Classic)?.cssVariables).toMatchObject({
      '--app-accent': 'rgb(1, 101, 221)',
      '--app-bg-primary': 'rgb(45, 45, 45)',
      '--app-text-primary': 'rgb(226, 226, 226)',
    });
  });
});
