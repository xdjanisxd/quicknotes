export const themes = [
  { id: 'system', label: 'System' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
  { id: 'dracula', label: 'Dracula' },
  { id: 'nord', label: 'Nord' },
  { id: 'catppuccin', label: 'Catppuccin Mocha' },
] as const;

export type Theme = typeof themes[number]['id'];

export function currentTheme(): Theme {
  const value = document.documentElement.dataset.theme;
  return themes.find(theme => theme.id === value)?.id ?? 'system';
}

export function chooseTheme(value: string) {
  const theme = themes.find(theme => theme.id === value);
  if (!theme) throw new Error('Unknown theme.');
  // Save before applying so a storage failure never looks like a saved preference.
  localStorage.setItem('quicknote.theme', theme.id);
  document.documentElement.dataset.theme = theme.id;
}
