import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { installDirectionalNavigation } from './lib/navigation';
import '@fontsource/be-vietnam-pro/400.css';
import '@fontsource/be-vietnam-pro/500.css';
import '@fontsource/be-vietnam-pro/600.css';
import '@fontsource/be-vietnam-pro/700.css';
import './styles.css';

installDirectionalNavigation();
ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode><App /></React.StrictMode>);
