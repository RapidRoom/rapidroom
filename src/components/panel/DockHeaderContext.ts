import { createContext } from 'react';

export const DockHeaderContext = createContext<{ element: HTMLDivElement | null; onToggle: () => void } | null>(null);
