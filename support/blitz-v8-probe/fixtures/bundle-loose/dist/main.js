// Packaged-layout fixture: proves the entry document and its module load from
// game://content/ (src/launch.rs). The JSON module is never named by the
// entry document, so in the packed copy (../bundle-packed) it sits in a
// non-boot archive extracted on first access.
import data from "./data/lazy.json" with { type: "json" };

document.getElementById("root").textContent = `bundle ok ${location.href} ${data.lazy}`;
