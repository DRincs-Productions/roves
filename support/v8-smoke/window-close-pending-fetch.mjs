// Static dependency held by window_close_pending_fetch.py's loopback server. The report below
// runs only if the dependency was delivered and evaluated before shutdown: a diagnostic, since
// the release races with the posted WM_CLOSE by design.
import { held } from "/window-close-held.mjs";

fetch("/window-close-evaluated?held=" + held).catch(() => {});
