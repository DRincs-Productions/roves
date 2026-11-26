// Roves local MCP server (stdio) for a running Roves CEF game on Windows, macOS or Linux.
// See README.md here and "Local MCP bridge" in support/cef-netrender-probe/README.md.
//
// The game publishes <session directory>/<pid>.json at launch when its bridge is on
// (--roves-mcp, or the package marker mcp/enabled). The session directory is per user:
//   Windows  %LOCALAPPDATA%\Roves\mcp
//   macOS    $HOME/Library/Application Support/Roves/mcp
//   Linux    $XDG_RUNTIME_DIR/roves/mcp, else $HOME/.local/state/roves/mcp
// (the same rule as the game's mcp_rules.rs `record_dir`). Every tool call rereads those
// records, so the game may start after this server; the call goes to the game's local
// endpoint (a named pipe on Windows, a unix-domain socket in the session directory on
// macOS/Linux) after the session token. No TCP port is used.
import { McpServer } from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { createConnection } from "node:net";
import { isAbsolute, join } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";
import { z } from "zod";

const args = process.argv.slice(2);
let pinned;
if (args.length === 2 && args[0] === "--pid" && /^[1-9]\d*$/.test(args[1])) {
  pinned = Number(args[1]);
} else if (args.length) {
  throw new Error("Usage: node server.mjs [--pid <game process id>]");
}

const PIPE = /^\\\\\.\\pipe\\roves-mcp-([1-9]\d*)-[0-9a-f]{32}$/;
const TOKEN = /^[0-9a-f]{64}$/;
const KEYS = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Enter", "Escape", "Space",
  "Tab", "Backspace", "Delete", "Home", "End", "PageUp", "PageDown"];

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    // EPERM: the process exists but belongs to someone else, so it is not this user's game.
    return false;
  }
}

/** The per-user session directory of this platform (mcp_rules.rs `record_dir`). */
function sessionDirectory(platform = process.platform, env = process.env) {
  const absolute = (name) => (env[name] && isAbsolute(env[name]) ? env[name] : undefined);
  if (platform === "win32") {
    if (!env.LOCALAPPDATA) throw new Error("Roves MCP needs a Windows user profile (LOCALAPPDATA)");
    return join(env.LOCALAPPDATA, "Roves", "mcp");
  }
  if (platform === "darwin") {
    if (!absolute("HOME")) throw new Error("Roves MCP needs HOME (an absolute path)");
    return join(env.HOME, "Library", "Application Support", "Roves", "mcp");
  }
  if (platform === "linux") {
    if (absolute("XDG_RUNTIME_DIR")) return join(env.XDG_RUNTIME_DIR, "roves", "mcp");
    if (!absolute("HOME")) throw new Error("Roves MCP needs XDG_RUNTIME_DIR or HOME (an absolute path)");
    return join(env.HOME, ".local", "state", "roves", "mcp");
  }
  throw new Error(`Roves MCP is not available on ${platform}`);
}

/** Whether `entry.pipe` is the endpoint a game with this PID publishes on this platform. */
function validEndpoint(entry, directory) {
  if (process.platform === "win32") {
    const pipe = PIPE.exec(entry.pipe ?? "");
    return Boolean(pipe) && Number(pipe[1]) === entry.pid;
  }
  // macOS/Linux: the socket <pid>.sock in the same private directory.
  return entry.pipe === join(directory, `${entry.pid}.sock`);
}

/** Running sessions of this user, newest record format only. */
function sessions() {
  const directory = sessionDirectory();
  let files;
  try {
    files = readdirSync(directory);
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
  const found = [];
  for (const file of files) {
    const match = /^([1-9]\d*)\.json$/.exec(file);
    if (!match) continue;
    try {
      const path = join(directory, file);
      // macOS/Linux: the directory is private (0700); a record not owned by this user is
      // not one of this user's games.
      if (process.getuid && statSync(path).uid !== process.getuid()) continue;
      const entry = JSON.parse(readFileSync(path, "utf8"));
      if (entry.version !== 1 || entry.pid !== Number(match[1]) || !validEndpoint(entry, directory) ||
          !TOKEN.test(entry.token ?? "") || typeof entry.origin !== "string" || !alive(entry.pid)) continue;
      found.push(entry);
    } catch {
      // A record can disappear or be half-replaced while the directory is read.
    }
  }
  return found;
}

function select() {
  const running = sessions().filter(({ pid }) => pinned === undefined || pid === pinned);
  if (!running.length) {
    throw new Error(pinned === undefined
      ? "No Roves game with MCP enabled is running. Start it with --roves-mcp (or an MCP-enabled package)."
      : `The Roves game with PID ${pinned} is not running with MCP enabled.`);
  }
  if (running.length > 1) {
    throw new Error("Several Roves games are running. Restart this server with --pid <PID>, one of: " +
      running.map(({ pid }) => pid).join(", "));
  }
  return running[0];
}

/** Sends one request to the game and returns its value. */
async function request(method, params = {}) {
  const session = select();
  const socket = createConnection(session.pipe);
  socket.setTimeout(15_000, () => socket.destroy(new Error("The Roves game did not answer in time")));
  const lines = createInterface({ input: socket });
  try {
    socket.write(session.token + "\n" + JSON.stringify({ method, params }) + "\n");
    for await (const line of lines) {
      const reply = JSON.parse(line);
      if ("error" in reply) throw new Error(reply.error);
      return reply.value;
    }
    throw new Error("The Roves game closed the connection without an answer");
  } finally {
    lines.close();
    socket.destroy();
  }
}

// Discovery at launch: report what is already running (stderr; stdout carries MCP).
try {
  const running = sessions();
  process.stderr.write(running.length
    ? `roves-mcp: found ${running.map(({ pid, origin }) => `pid=${pid} origin=${origin}`).join("; ")}\n`
    : "roves-mcp: no running game yet; tools look again on every call\n");
} catch (error) {
  process.stderr.write(`roves-mcp: ${error.message}\n`);
}

// Page probes: self-contained functions sent as one `evaluate` expression each (source plus
// JSON arguments, well under the bridge's 16 KiB limit), so they run with the page's own
// privileges and nothing is installed in the page. Coordinates are CSS pixels of the
// top-level viewport, i.e. the window points `click` takes (page zoom 1).

/** Visible text and the visible controls a player could click or type into. */
function pageSnapshot({ maxItems, maxText }) {
  const vw = innerWidth, vh = innerHeight;
  const squash = (s) => String(s ?? "").replace(/\s+/g, " ").trim();
  const shown = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1 || r.bottom <= 0 || r.right <= 0 || r.top >= vh || r.left >= vw) return null;
    const s = getComputedStyle(el);
    return s.visibility === "hidden" || s.display === "none" || Number(s.opacity) === 0 ? null : r;
  };
  const found = new Set(document.querySelectorAll("a[href],button,input,select,textarea,summary,label[for]," +
    "[role=button],[role=link],[role=menuitem],[role=tab],[role=checkbox],[role=radio],[role=switch]," +
    "[role=slider],[role=option],[onclick],[contenteditable=''],[contenteditable=true],[tabindex]:not([tabindex='-1'])"));
  // Framework click handlers leave no attribute: take the outermost cursor:pointer element.
  let scanned = 0;
  for (const el of document.body?.querySelectorAll("*") ?? []) {
    if (++scanned > 4000) break;
    if (getComputedStyle(el).cursor === "pointer" &&
        !(el.parentElement && getComputedStyle(el.parentElement).cursor === "pointer")) found.add(el);
  }
  const items = [];
  for (const el of found) {
    const r = shown(el);
    if (!r) continue;
    const x = Math.round(Math.min(Math.max(r.left + r.width / 2, 0), vw - 1));
    const y = Math.round(Math.min(Math.max(r.top + r.height / 2, 0), vh - 1));
    const hit = document.elementFromPoint(x, y);
    const item = { tag: el.tagName.toLowerCase(), x, y, w: Math.round(r.width), h: Math.round(r.height) };
    const role = el.getAttribute("role") || (el.type && el.tagName === "INPUT" ? el.type : "");
    if (role) item.role = role;
    const text = squash(el.getAttribute("aria-label") || el.innerText || el.value || el.placeholder || el.title || el.alt).slice(0, 80);
    if (text) item.text = text;
    if (el.id) item.id = el.id;
    if (el.disabled || el.getAttribute("aria-disabled") === "true") item.disabled = true;
    if (hit && hit !== el && !el.contains(hit)) item.covered_by = hit.tagName.toLowerCase() + (hit.id ? "#" + hit.id : "");
    items.push(item);
  }
  items.sort((a, b) => a.y - b.y || a.x - b.x);
  const canvases = [...document.querySelectorAll("canvas")].map((c) => [c, shown(c)]).filter(([, r]) => r)
    .map(([c, r]) => ({ id: c.id || undefined, x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width),
      h: Math.round(r.height), backing: `${c.width}x${c.height}` }));
  const active = document.activeElement;
  return {
    url: location.href, title: document.title, viewport: { width: vw, height: vh, dpr: devicePixelRatio },
    focused: active && active !== document.body ? active.tagName.toLowerCase() + (active.id ? "#" + active.id : "") : null,
    text: squash(document.body?.innerText).slice(0, maxText),
    controls: items.slice(0, maxItems), more_controls: Math.max(0, items.length - maxItems), canvases,
  };
}

/** One element (by selector, or the topmost at a window point): markup, box, styles, CSS rules. */
function inspectElement({ selector, x, y, maxHtml, styles, maxRules }) {
  const el = selector ? document.querySelector(selector) : document.elementFromPoint(x, y);
  if (!el) return { found: false };
  const r = el.getBoundingClientRect();
  const cs = getComputedStyle(el);
  const describe = (n) => n.tagName.toLowerCase() + (n.id ? "#" + n.id : "") +
    [...n.classList].slice(0, 4).map((c) => "." + c).join("");
  const ancestors = [];
  for (let n = el.parentElement; n && ancestors.length < 6; n = n.parentElement) ancestors.push(describe(n));
  const computed = {};
  for (const name of styles) {
    const value = cs.getPropertyValue(name);
    if (value) computed[name] = value;
  }
  const rules = [];
  const source = (sheet) => sheet.href ? sheet.href.replace(location.origin, "") :
    sheet.ownerNode ? `<${sheet.ownerNode.tagName.toLowerCase()}>` : "constructed";
  const walk = (list, sheet, condition) => {
    for (const rule of list) {
      if (rules.length >= maxRules) return;
      if (rule.selectorText !== undefined) {
        let match = false;
        try { match = el.matches(rule.selectorText); } catch {}
        if (match) rules.push({ selector: rule.selectorText, source: source(sheet), ...(condition ? { at: condition } : {}),
          css: rule.style.cssText.slice(0, 400) });
      } else if (rule.cssRules) {
        if (rule.media && !matchMedia(rule.media.mediaText).matches) continue;
        if (rule.conditionText && rule.constructor.name === "CSSSupportsRule" && !CSS.supports(rule.conditionText)) continue;
        const label = rule.media ? "@media " + rule.media.mediaText : rule.conditionText ?? rule.name;
        walk(rule.cssRules, sheet, label ? [condition, label].filter(Boolean).join(" ") : condition);
      }
    }
  };
  for (const sheet of [...document.styleSheets, ...(document.adoptedStyleSheets ?? [])]) {
    let list;
    try { list = sheet.cssRules; } catch { rules.push({ source: source(sheet), unreadable: true }); continue; }
    walk(list, sheet, "");
  }
  const html = el.outerHTML;
  const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
  return {
    found: true, element: describe(el), matches: selector ? document.querySelectorAll(selector).length : undefined,
    box: { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) },
    covered_by: hit && hit !== el && !el.contains(hit) ? describe(hit) : undefined,
    ancestors, children: el.children.length,
    text: (el.innerText ?? "").replace(/\s+/g, " ").trim().slice(0, 200),
    html: html.length > maxHtml ? html.slice(0, maxHtml) + `... (${html.length} chars)` : html,
    inline: el.getAttribute("style") ?? undefined, computed,
    rules, rules_note: "document order: for equal specificity a later rule wins; inline style beats both",
  };
}

/**
 * The PixiJS scene, found the way the @pixi/devtools backend finds it: `__PIXI_DEVTOOLS__`
 * (`initDevtools()`, which Pixi'VN's Game.init calls), then `__PIXI_APP__`, `__PIXI_STAGE__`,
 * `__PIXI_RENDERER__`, in this window and its same-origin frames.
 */
function pixiScene({ path, depth, maxNodes, find, at, all }) {
  const lookup = (key) => {
    if (globalThis[key]) return globalThis[key];
    for (let i = 0; i < frames.length; i += 1) {
      try { if (frames[i][key]) return frames[i][key]; } catch {}
    }
  };
  const devtools = lookup("__PIXI_DEVTOOLS__");
  const app = devtools?.app ?? lookup("__PIXI_APP__");
  const renderer = devtools?.renderer ?? lookup("__PIXI_RENDERER__") ?? app?.renderer;
  const stage = devtools?.stage ?? lookup("__PIXI_STAGE__") ?? app?.stage ?? renderer?.lastObjectRendered;
  if (!stage) {
    return { detected: false, hint: "No PixiJS scene is exposed. Call initDevtools({ app }) from @pixi/devtools " +
      "(Pixi'VN does it in Game.init) or set globalThis.__PIXI_APP__ = app in a development build." };
  }
  const canvas = renderer?.canvas ?? renderer?.view;
  const rect = canvas?.getBoundingClientRect?.();
  const screen = renderer?.screen;
  const sx = rect && screen?.width ? rect.width / screen.width : 1, sy = rect && screen?.height ? rect.height / screen.height : 1;
  const ox = rect?.left ?? 0, oy = rect?.top ?? 0;
  const round = (v) => Math.round(v * 10) / 10;
  const kind = (n) => n.renderPipeId ?? (n.gotoAndPlay ? "animatedSprite" : n.text !== undefined ? "text" :
    n.texture ? "sprite" : n.geometry ? "graphics" : "container");
  const label = (n) => n.label ?? n.name ?? undefined;
  const window_box = (n) => {
    try {
      const b = n.getBounds();
      const box = b.rectangle ?? b;
      return { x: round(ox + box.x * sx), y: round(oy + box.y * sy), w: round(box.width * sx), h: round(box.height * sy) };
    } catch { return undefined; }
  };
  const describe = (n, p) => {
    const out = { path: p, type: kind(n) };
    const name = label(n);
    if (name) out.label = String(name).slice(0, 60);
    const cls = n.constructor?.name;
    if (cls && cls.length > 2 && cls.toLowerCase() !== out.type.toLowerCase()) out.class = cls;
    if (n.visible === false) out.visible = false;
    if (n.renderable === false) out.renderable = false;
    if (n.alpha !== undefined && n.alpha !== 1) out.alpha = round(n.alpha);
    if (n.x || n.y) out.pos = [round(n.x), round(n.y)];
    if (n.scale && (n.scale.x !== 1 || n.scale.y !== 1)) out.scale = [round(n.scale.x), round(n.scale.y)];
    if (n.rotation) out.rotation = round(n.rotation);
    if (n.zIndex) out.zIndex = n.zIndex;
    if (typeof n.text === "string") out.text = n.text.slice(0, 80);
    const texture = n.texture && (n.texture.label ?? n.texture.source?.label ?? n.texture.source?.resource?.src ??
      n.texture.baseTexture?.resource?.url ?? n.texture.textureCacheIds?.[0]);
    if (texture) out.texture = String(texture).slice(-80);
    if (n.eventMode && n.eventMode !== "passive" && n.eventMode !== "auto") out.eventMode = n.eventMode;
    else if (n.interactive) out.eventMode = "static";
    if (n.filters?.length) out.filters = n.filters.length;
    if (n.mask) out.mask = true;
    const box = window_box(n);
    if (box) out.window = box;
    if (n.children?.length) out.children = n.children.length;
    return out;
  };
  const shown = (n) => n.visible !== false && n.renderable !== false && (n.alpha ?? 1) > 0;
  let root = stage;
  for (const part of path ? String(path).split("/").filter(Boolean) : []) {
    root = root?.children?.[Number(part)];
    if (!root) return { detected: true, error: `no node at path ${path}` };
  }
  const nodes = [];
  let visited = 0, truncated = false;
  const visit = (n, p, level, worldVisible) => {
    if (++visited > 20000) { truncated = true; return; }
    const visible = worldVisible && shown(n);
    let keep;
    if (at) {
      const box = visible && window_box(n);
      keep = box && at.x >= box.x && at.x <= box.x + box.w && at.y >= box.y && at.y <= box.y + box.h;
    } else if (find) {
      const needle = find.toLowerCase();
      keep = [label(n), n.text, n.texture?.label, n.texture?.source?.label, kind(n), n.constructor?.name]
        .some((v) => typeof v === "string" && v.toLowerCase().includes(needle));
    } else {
      keep = level <= depth && (all || visible);
    }
    if (keep) {
      if (nodes.length >= maxNodes) { truncated = true; return; }
      nodes.push({ depth: level, ...describe(n, p || "/") });
    }
    if (!at && !find && level >= depth) return;
    if (!all && !visible && !find) return;
    (n.children ?? []).forEach((c, i) => visit(c, (p ? p + "/" : "") + i, level + 1, visible));
  };
  visit(root, path ? String(path).replace(/^\/+|\/+$/g, "") : "", 0, true);
  if (at) nodes.reverse();
  return {
    detected: true,
    source: devtools ? "__PIXI_DEVTOOLS__" : lookup("__PIXI_APP__") ? "__PIXI_APP__" : lookup("__PIXI_STAGE__") ? "__PIXI_STAGE__" : "__PIXI_RENDERER__",
    version: devtools?.version ?? devtools?.pixi?.VERSION ?? globalThis.PIXI?.VERSION ?? undefined,
    renderer: renderer && {
      type: renderer.name ?? ({ 1: "webgl", 2: "canvas", 4: "webgpu" })[renderer.type] ?? renderer.type,
      resolution: renderer.resolution, screen: screen && [screen.width, screen.height],
      canvas: canvas && { backing: [canvas.width, canvas.height], window: rect && [round(rect.left), round(rect.top), round(rect.width), round(rect.height)] },
    },
    fps: app?.ticker ? round(app.ticker.FPS) : undefined,
    nodes, truncated,
    note: "path: child indexes from the stage (or the given path); window: bounds in window points for click",
  };
}

/** One evaluate expression calling `probe` with `args`. */
const probeExpression = (probe, args) => `(${probe.toString()})(${JSON.stringify(args)})`;

const DEFAULT_STYLES = ["display", "position", "z-index", "top", "left", "width", "height", "margin", "padding",
  "box-sizing", "overflow", "color", "background-color", "background-image", "font-family", "font-size",
  "font-weight", "line-height", "text-align", "opacity", "visibility", "transform", "pointer-events", "cursor",
  "flex-direction", "justify-content", "align-items", "gap", "border", "border-radius", "box-shadow", "filter"];

const region = z.object({
  x: z.number(), y: z.number(), width: z.number().positive(), height: z.number().positive(),
}).describe("Part of the window in window points (CSS pixels); default the whole window");

/** One screenshot from the game: the bridge's image and its geometry. */
async function capture({ format = "jpeg", quality = 60, max_width = 640, region: area } = {}) {
  const shot = await request("screenshot", { format, quality, max_width, ...(area ? { region: area } : {}) });
  const width = Math.round(shot.region.width * shot.scale);
  const height = Math.round(shot.region.height * shot.scale);
  return { ...shot, width, height };
}

const imageMeta = (shot) => ({
  size: [shot.width, shot.height], region: shot.region,
  to_window: `window point = (${shot.region.x} + px / ${+shot.scale.toFixed(4)}, ${shot.region.y} + py / ${+shot.scale.toFixed(4)})`,
});

await serveStdio(() => {
  const server = new McpServer({ name: "roves-cef", version: "0.0.0" });
  const tool = (name, description, inputSchema, call) => server.registerTool(name,
    { description, inputSchema },
    async (input) => {
      try {
        const value = await call(input);
        if (value?.content) return value;
        return { content: [{ type: "text", text: JSON.stringify(value) }] };
      } catch (error) {
        return { isError: true, content: [{ type: "text", text: error.message }] };
      }
    });
  tool("get_state",
    "Read the running Roves game's host-side state: origin, URL, title, load/startup state, " +
    "whether input is accepted, window size in points, frame counters and recent console lines.",
    z.object({}), () => request("state"));
  tool("evaluate_script",
    "Evaluate a synchronous JavaScript expression in the game page and return its JSON value. " +
    "Promises are refused: start asynchronous work in one call and read its result later. " +
    "The page's Content Security Policy still applies to eval inside the expression. " +
    "Trusted development command: it can change game state.",
    z.object({ expression: z.string().min(1).max(16 * 1024) }),
    ({ expression }) => request("evaluate", { expression }));
  tool("click",
    "Left-click at a window point (x, y in window points = CSS pixels at page zoom 1, origin top-left), " +
    "through the same input path as a real mouse click.",
    z.object({ x: z.number().nonnegative(), y: z.number().nonnegative() }),
    ({ x, y }) => request("click", { x, y }));
  tool("press_key",
    "Press and release one non-character key (keydown + keyup, no character input).",
    z.object({ key: z.enum(KEYS) }),
    ({ key }) => request("press_key", { key }));
  tool("type_text",
    "Type up to 256 printable characters as text input into the focused page element.",
    z.object({ text: z.string().min(1).max(256) }),
    ({ text }) => request("type_text", { text }));

  const shotOptions = (quality) => ({
    format: z.enum(["jpeg", "webp", "png"]).default("jpeg")
      .describe("jpeg/webp are small; png only for exact pixels"),
    quality: z.number().int().min(1).max(100).default(quality).describe("jpeg/webp quality"),
    region: region.optional(),
  });
  tool("screenshot",
    "Capture what the game window shows now (the page as composited by the browser, canvas " +
    "included), scaled to at most max_width pixels. Image cost grows with its area (about " +
    "width x height / 750 tokens): keep max_width small to look at the whole screen and use " +
    "region to read details. The text part maps image pixels back to window points for click.",
    z.object({ max_width: z.number().int().min(16).max(4096).default(640), ...shotOptions(60) }),
    async (input) => {
      const shot = await capture(input);
      return { content: [
        { type: "image", data: shot.data, mimeType: shot.mime },
        { type: "text", text: JSON.stringify(imageMeta(shot)) },
      ] };
    });
  tool("watch",
    "Capture a short sequence of frames, optionally right after one input action, to see an " +
    "animation, transition or reaction that is over before a separate screenshot call. Frames " +
    "identical to the previous one are listed but not returned (skip_unchanged). Timestamps " +
    "are real: each capture takes a round trip to the game, so interval_ms is a minimum.",
    z.object({
      count: z.number().int().min(1).max(20).default(6),
      interval_ms: z.number().int().min(0).max(5000).default(200),
      max_width: z.number().int().min(16).max(1280).default(320),
      ...shotOptions(50),
      skip_unchanged: z.boolean().default(true),
      action: z.union([
        z.object({ click: z.object({ x: z.number().nonnegative(), y: z.number().nonnegative() }) }),
        z.object({ key: z.enum(KEYS) }),
        z.object({ text: z.string().min(1).max(256) }),
      ]).optional().describe("Input sent once just before the first frame"),
    }),
    async ({ count, interval_ms, skip_unchanged, action, ...options }) => {
      const start = Date.now();
      if (action?.click) await request("click", action.click);
      else if (action?.key) await request("press_key", { key: action.key });
      else if (action?.text) await request("type_text", { text: action.text });
      const content = [];
      const frames = [];
      let previous, geometry;
      for (let index = 0; index < count && Date.now() - start < 30_000; index += 1) {
        if (index) await delay(interval_ms);
        const shot = await capture(options);
        const t_ms = Date.now() - start;
        if (skip_unchanged && shot.data === previous?.data) {
          frames.push({ frame: index, t_ms, same_as: previous.frame });
          continue;
        }
        previous = { data: shot.data, frame: index };
        frames.push({ frame: index, t_ms, image: content.length });
        content.push({ type: "image", data: shot.data, mimeType: shot.mime });
        geometry ??= imageMeta(shot);
      }
      content.push({ type: "text", text: JSON.stringify({ frames, ...geometry }) });
      return { content };
    });
  tool("page_snapshot",
    "Read the current page as text: visible text, the visible controls a player can click or " +
    "type into (tag, text, center x/y in window points for click, size, whether something " +
    "covers it) and the canvases. Much cheaper than a screenshot for DOM menus and dialogue; " +
    "a canvas game draws its content inside the canvas, so use pixi_scene or screenshot there.",
    z.object({
      max_items: z.number().int().min(1).max(300).default(80),
      max_text: z.number().int().min(0).max(20000).default(2000),
    }),
    ({ max_items, max_text }) => request("evaluate", {
      expression: probeExpression(pageSnapshot, { maxItems: max_items, maxText: max_text }),
    }));
  tool("inspect_element",
    "Inspect one element's HTML and CSS, chosen by CSS selector or as the topmost element at a " +
    "window point (x, y): truncated outerHTML, box, ancestors, what covers it, computed styles " +
    "(a default layout/visual set or the given property names) and the CSS rules of the page's " +
    "same-origin stylesheets that match it, with their source and @media/@supports conditions.",
    z.object({
      selector: z.string().min(1).max(1000).optional(),
      x: z.number().nonnegative().optional(),
      y: z.number().nonnegative().optional(),
      styles: z.array(z.string().min(1).max(64)).max(80).optional(),
      max_html: z.number().int().min(0).max(20000).default(1500),
      max_rules: z.number().int().min(0).max(200).default(40),
    }),
    ({ selector, x, y, styles, max_html, max_rules }) => {
      if (!selector && (x === undefined || y === undefined)) throw new Error("Give selector, or x and y");
      return request("evaluate", { expression: probeExpression(inspectElement, {
        selector, x, y, maxHtml: max_html, styles: styles ?? DEFAULT_STYLES, maxRules: max_rules,
      }) });
    });
  tool("pixi_scene",
    "Read the PixiJS scene graph when the game exposes it (@pixi/devtools initDevtools, which " +
    "Pixi'VN calls, or __PIXI_APP__/__PIXI_STAGE__/__PIXI_RENDERER__): renderer, FPS and a " +
    "bounded flat list of nodes with path, type, label, text, texture, transform, visibility, " +
    "interactivity and bounds in window points. Modes: the visible tree under path to depth; " +
    "find (label/text/texture/type substring over the whole tree); at (nodes under a window " +
    "point, topmost first). Change nodes with evaluate_script.",
    z.object({
      path: z.string().max(200).optional().describe("Child indexes from the stage, e.g. \"0/3\""),
      depth: z.number().int().min(0).max(30).default(3),
      max_nodes: z.number().int().min(1).max(500).default(120),
      find: z.string().min(1).max(100).optional(),
      at: z.object({ x: z.number(), y: z.number() }).optional(),
      all: z.boolean().default(false).describe("Include hidden nodes and their subtrees"),
    }),
    ({ path, depth, max_nodes, find, at, all }) => request("evaluate", {
      expression: probeExpression(pixiScene, { path, depth, maxNodes: max_nodes, find, at, all }),
    }));
  return server;
});
