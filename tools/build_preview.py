"""Inlines CSS/JS into single-file HTML previews (with a fake Tauri bridge and
sample settings) so the UI can be checked in any browser without building.

  python3 tools/build_preview.py [out_dir] [path/to/dot-clock]

If the Dot Clock project is given, also writes launcher-side-by-side.html showing the styles under Dot Clock
widgets together, with Dot Launcher matching Dot Clock's look."""
import html, json, os, re, sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "preview")
WT_ROOT = sys.argv[2] if len(sys.argv) > 2 else None
os.makedirs(OUT, exist_ok=True)

WORLD_TIME_SAMPLE = {
    "clocks": [
        {"id": "h", "label": "London, UK", "tz": "Europe/London", "lat": 51.51, "lng": -0.13, "home": True},
        {"id": "a", "label": "New York, USA", "tz": "America/New_York", "lat": 40.71, "lng": -74.01},
        {"id": "b", "label": "Tokyo, Japan", "tz": "Asia/Tokyo", "lat": 35.68, "lng": 139.69},
    ],
    "use24h": True, "dotColor": "#ffb000", "dotSize": 3, "bgColor": "#101010", "bgOpacity": 0.85,
}

TIMER_SAMPLE_JS = """({ matchWorldTime: %(match)s, style: '%(style)s', rows: %(rows)s, items: [
    { id: 'i0', name: 'Chrome', target: 'C:/Apps/Chrome.lnk', kind: 'app', icon: 'globe', color: '#5ad1ff' },
    { id: 'i1', name: 'Outlook', target: 'C:/Apps/Outlook.lnk', kind: 'app', icon: 'mail', color: '#5ad1ff' },
    { id: 'i2', name: 'Teams', target: 'C:/Apps/Teams.lnk', kind: 'app', icon: 'chat', color: '#c9a0ff' },
    { id: 'i3', name: 'Spotify', target: 'C:/Apps/Spotify.lnk', kind: 'app', icon: 'music', color: '#39ff7a' },
    { id: 'i4', name: 'VS Code', target: 'C:/Apps/Code.lnk', kind: 'app', icon: 'code', color: '#5ad1ff' },
    { id: 'i5', name: 'Terminal', target: 'C:/Apps/Terminal.lnk', kind: 'app', icon: 'terminal', color: '#e8e4d0' },
    { id: 'i6', name: 'Calculator', target: 'C:/Apps/Calc.lnk', kind: 'app', icon: 'calc', color: '#ffb000' },
    { id: 'i7', name: 'Downloads', target: 'C:/Users/You/Downloads', kind: 'folder', icon: 'folder', color: '#ffb000' },
    { id: 'i8', name: 'Projects', target: 'C:/Users/You/Projects', kind: 'folder', icon: 'folder', color: '#b8ff5a' },
    { id: 'i9', name: 'Photos', target: 'C:/Users/You/Pictures', kind: 'folder', icon: 'photo', color: '#ff6bd5' },
    { id: 'i10', name: 'BBC News', target: 'https://www.bbc.co.uk/news', kind: 'web', icon: 'globe', color: '#ff5a4a' },
    { id: 'i11', name: 'Budget', target: 'C:/Users/You/Budget.xlsx', kind: 'file', icon: 'doc', color: '#39ff7a' },
    { id: 'i12', name: 'Steam', target: 'C:/Apps/Steam.lnk', kind: 'app', icon: 'game', color: '#e8e4d0' },
    { id: 'i13', name: 'Settings', target: 'ms-settings:', kind: 'app', icon: 'gear', color: '#e8e4d0' } ] })"""


def shim(match=True, style="icons", rows=2):
    state_js = TIMER_SAMPLE_JS % {"match": "true" if match else "false", "style": style, "rows": rows}
    return """<script>
var __state = JSON.stringify(%s);
window.__TAURI__ = {
  core: { invoke: async (cmd, args) => {
    if (cmd === 'load_state') return __state;
    if (cmd === 'read_world_time') return %s;
    if (cmd === 'save_state') { __state = args.json; return; }
    if (cmd === 'launch') return new Promise((r) => setTimeout(r, 200));
    if (cmd === 'pick_path') return args.kind === 'folder' ? 'C:\\\\Users\\\\You\\\\Music' : 'C:\\\\ProgramData\\\\Microsoft\\\\Windows\\\\Start Menu\\\\Programs\\\\Notepad.lnk';
  } },
  event: { listen: async () => () => {} },
  window: { getCurrentWindow: () => ({ setSize: async () => {}, setAlwaysOnTop: async () => {}, startDragging: async () => {} }) },
  dpi: { LogicalSize: function (w, h) { this.width = w; this.height = h; } },
};
</script>""" % (state_js, json.dumps(json.dumps(WORLD_TIME_SAMPLE)))


def inline(root, name, head=""):
    src = open(os.path.join(root, "src", name), encoding="utf-8").read()
    src = re.sub(r'<link rel="stylesheet" href="([^"]+)">',
                 lambda m: "<style>" + open(os.path.join(root, "src", m.group(1)), encoding="utf-8").read() + "</style>", src)
    src = re.sub(r'<script src="([^"]+)"></script>',
                 lambda m: "<script>" + open(os.path.join(root, "src", m.group(1)), encoding="utf-8").read()
                 .replace("</script", "<\\/script") + "</script>", src)
    return src.replace("</head>", head + "</head>", 1)


DESK = "<style>body{min-height:100vh;padding:24px;background:linear-gradient(135deg,#3a6186,#89253e)!important}</style>"
open(os.path.join(OUT, "launcher-widget-preview.html"), "w", encoding="utf-8").write(inline(ROOT, "index.html", DESK + shim()))
open(os.path.join(OUT, "launcher-settings-preview.html"), "w", encoding="utf-8").write(inline(ROOT, "settings.html", shim()))

if WT_ROOT:
    wt_shim = """<script>window.__TAURI__ = { core: { invoke: async (c) => c === 'load_state' ? %s : undefined },
      event: { listen: async () => () => {} },
      window: { getCurrentWindow: () => ({ setSize: async () => {}, setAlwaysOnTop: async () => {}, startDragging: async () => {} }) },
      dpi: { LogicalSize: function () {} } };</script>""" % json.dumps(json.dumps(WORLD_TIME_SAMPLE))
    wt_doc = inline(WT_ROOT, "index.html", wt_shim)
    styles = [("icons", 1, "A · Icons, 1 row", 100), ("icons", 2, "A · Icons, 2 rows", 155), ("icons", 3, "A · Icons, 3 rows", 210), ("list", 2, "B · List", 175)]
    frames = "".join('<div class="cap">%s</div><iframe style="height:%dpx" srcdoc="%s"></iframe>\n'
                     % (t, h, html.escape(inline(ROOT, "index.html", shim(True, sid, rows)), quote=True)) for sid, rows, t, h in styles)
    page = """<!doctype html><html><head><meta charset="utf-8"><title>Side by side</title>
<style>body{margin:0;min-height:100vh;padding:24px;box-sizing:border-box;background:linear-gradient(135deg,#3a6186,#89253e);
font:12px system-ui;color:#fff}iframe{border:0;display:block;background:transparent;width:100%%;}
.cap{opacity:.75;margin:12px 0 4px}</style></head><body>
<div class="cap">Dot Clock</div><iframe id="a" style="height:100px" srcdoc="%s"></iframe>
%s
<div class="cap">Dot Launcher (matching Dot Clock). Hover for names, click to "open".</div>
</body></html>""" % (html.escape(wt_doc, quote=True), frames)
    open(os.path.join(OUT, "launcher-side-by-side.html"), "w", encoding="utf-8").write(page)

print("previews written to", os.path.abspath(OUT))
