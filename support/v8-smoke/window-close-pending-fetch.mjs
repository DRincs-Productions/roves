// Static dependency held by window_close_pending_fetch.py's loopback server. The report below
// runs only if the dependency was delivered and evaluated before shutdown. The harness requires
// the process to exit while this response is still held, so this report should not be sent.
import { held } from "/window-close-held.mjs";

fetch("/window-close-evaluated?held=" + held).catch(() => {});
