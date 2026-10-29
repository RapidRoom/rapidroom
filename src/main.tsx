import React from 'react';
import { createRoot } from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './App';
import ViewerWindow from './window/ViewerWindow';
import { installFrontendLogBridge } from './utils/frontendLogBridge';
import '@fontsource/poppins/400.css';
import '@fontsource/poppins/400-italic.css';
import '@fontsource/poppins/500.css';
import '@fontsource/poppins/500-italic.css';
import '@fontsource/poppins/600.css';
import '@fontsource/poppins/600-italic.css';
import '@fontsource/poppins/700.css';
import '@fontsource/poppins/700-italic.css';
import './styles.css';

installFrontendLogBridge();

const isViewerWindow = getCurrentWindow().label === 'viewer';

const root = createRoot(document.getElementById('root')!);
root.render(<React.StrictMode>{isViewerWindow ? <ViewerWindow /> : <App />}</React.StrictMode>);
