import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { windowLabel } from './tauri'
import Whiteout from './windows/Whiteout'
import SettingsWindow from './windows/Settings'

const label = windowLabel()
const Root = label.startsWith('whiteout') ? Whiteout : SettingsWindow
createRoot(document.getElementById('root')!).render(<StrictMode><Root /></StrictMode>)
