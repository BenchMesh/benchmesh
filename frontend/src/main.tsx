import { createRoot } from 'react-dom/client'
import App from './ui/App'
import { PanelWindow } from './ui/bench/PanelWindow'
import { SkinProvider } from './ui/kit/skin'
import { RequestLogProvider } from './ui/RequestLogContext'
import { suppressNativeContextMenu } from './ui/nativeMenu'
import { installSelectPopups } from './ui/kit/SelectPopup'

// right-click opens the app's own menus only (e.g. add a reading to a recording)
suppressNativeContextMenu()
// dropdown lists in the skin, not the system's (a GTK menu on Linux ignores the app's styles)
installSelectPopups()

const root = createRoot(document.getElementById('root')!)

// One instrument panel in its own window: /ui/?panel=<instrumentId:CLASS>
const panel = new URLSearchParams(window.location.search).get('panel')

if (panel) {
  root.render(<RequestLogProvider><SkinProvider><PanelWindow panelKey={panel} /></SkinProvider></RequestLogProvider>)
} else {
  root.render(<App />)
}