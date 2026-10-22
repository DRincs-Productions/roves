// Imported dynamically by gc-lifecycle.html from a classic script whose element was removed,
// and by two same-origin frames: importOwner() and evalImportOwner() must load gc-owner.js in
// its own frame's realm.
export default 42;
export function importOwner() { return import("./gc-owner.js"); }
// Direct eval code has no module record of its own: its import() must keep this module as
// referrer (owner realm and base URL), as the module's own import() does.
export function evalImportOwner() { return eval('import("./gc-owner.js")'); }
