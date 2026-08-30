import { CustomTheme } from './themeEngine';

/** Built-in theme presets. Users cannot delete these. */
export const BUILTIN_THEMES: CustomTheme[] = [
  {
    id: 'default-dark',
    name: 'Vault Dark',
    isDark: true,
    isBuiltin: true,
    palette: {
      bg: '#0b0c0f',
      surface: '#14161a',
      primary: '#e8a33d',
      secondary: '#7d9cc0',
      text: '#f2f3f5',
      subtext: '#969ea8',
      border: 'rgba(255, 255, 255, 0.08)',
      hover: 'rgba(255, 255, 255, 0.05)',
    },
  },
  {
    id: 'charcoal',
    name: 'Charcoal',
    isDark: true,
    isBuiltin: true,
    palette: {
      bg: '#1e1e2e',
      surface: '#282838',
      primary: '#6c63ff',
      secondary: '#a78bfa',
      text: '#e4e4ef',
      subtext: '#8888a8',
      border: 'rgba(255, 255, 255, 0.08)',
      hover: 'rgba(255, 255, 255, 0.04)',
    },
  },
  {
    id: 'nord',
    name: 'Nord',
    isDark: true,
    isBuiltin: true,
    palette: {
      bg: '#2e3440',
      surface: '#3b4252',
      primary: '#88c0d0',
      secondary: '#81a1c1',
      text: '#eceff4',
      subtext: '#a3b1c6',
      border: 'rgba(255, 255, 255, 0.08)',
      hover: 'rgba(255, 255, 255, 0.04)',
    },
  },
  {
    id: 'monokai',
    name: 'Monokai',
    isDark: true,
    isBuiltin: true,
    palette: {
      bg: '#272822',
      surface: '#2f302a',
      primary: '#a6e22e',
      secondary: '#66d9ef',
      text: '#f8f8f2',
      subtext: '#90908a',
      border: 'rgba(255, 255, 255, 0.08)',
      hover: 'rgba(255, 255, 255, 0.04)',
    },
  },
  {
    id: 'cyber-teal',
    name: 'Cyber Teal',
    isDark: true,
    isBuiltin: true,
    palette: {
      bg: '#0a1628',
      surface: '#112240',
      primary: '#00e5bf',
      secondary: '#00b4d8',
      text: '#e0f7f4',
      subtext: '#6faaaf',
      border: 'rgba(0, 229, 191, 0.12)',
      hover: 'rgba(0, 229, 191, 0.06)',
    },
  },
  {
    id: 'default-light',
    name: 'Vault Light',
    isDark: false,
    isBuiltin: true,
    palette: {
      bg: '#f4f4f2',
      surface: '#ffffff',
      primary: '#c98a1e',
      secondary: '#4a739f',
      text: '#17191d',
      subtext: '#6c7178',
      border: 'rgba(0, 0, 0, 0.09)',
      hover: 'rgba(0, 0, 0, 0.035)',
    },
  },
  {
    id: 'solarized-light',
    name: 'Solarized Light',
    isDark: false,
    isBuiltin: true,
    palette: {
      bg: '#fdf6e3',
      surface: '#eee8d5',
      primary: '#b58900',
      secondary: '#268bd2',
      text: '#073642',
      subtext: '#586e75',
      border: 'rgba(0, 0, 0, 0.1)',
      hover: 'rgba(0, 0, 0, 0.04)',
    },
  },
];

/** Default palette values to seed a new custom theme. */
export function getDefaultPalette(isDark: boolean) {
  const base = isDark
    ? BUILTIN_THEMES.find(t => t.id === 'default-dark')!
    : BUILTIN_THEMES.find(t => t.id === 'default-light')!;
  return { ...base.palette };
}
