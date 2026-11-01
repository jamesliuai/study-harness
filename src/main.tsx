import { createRoot } from 'react-dom/client';
import App from './App';
import { applyTheme, readTheme } from './theme';
import 'katex/dist/katex.min.css';
import './style.css';
applyTheme(readTheme());
createRoot(document.getElementById('root')!).render(<App />);
