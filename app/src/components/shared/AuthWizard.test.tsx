import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AuthWizard } from './AuthWizard';

const toggleTheme = vi.fn();
vi.mock('../../context/ThemeContext', () => ({
    useTheme: () => ({ theme: 'light', activeCustomThemeId: 'custom-dark', customThemes: [{ id: 'custom-dark', isDark: true }], toggleTheme }),
}));

describe('authentication theme control', () => {
    it('offers the opposite mode of the active custom theme', () => {
        Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
        render(<AuthWizard onLogin={() => {}} />);
        fireEvent.click(screen.getByRole('button', { name: 'Switch to Light Mode' }));
        expect(toggleTheme).toHaveBeenCalledTimes(1);
        delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    });
});
