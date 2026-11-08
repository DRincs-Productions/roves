// CSSOM View scrolling over Blitz's scroll state: viewport (window / root
// element) and an overflow element. Mirrors the visual-novel-template's
// narration scroll restoration (`scrollTo({top, left, behavior})`,
// `element.scrollTo({top, behavior: "smooth"})` centring through
// getBoundingClientRect + scrollTop - clientHeight / 2, hash `scrollIntoView`).
// Restores scroll 0,0 and removes its elements, so later stages are unaffected.
(() => {
  "use strict";
  const failures = [];
  const check = (condition, message) => {
    if (!condition) failures.push(message);
  };
  const near = (actual, expected) => Math.abs(actual - expected) <= 1;
  const throwsTypeError = (action) => {
    try {
      action();
    } catch (error) {
      return error instanceof TypeError;
    }
    return false;
  };
  const root = document.documentElement;

  // Viewport extent: a 1000x2000 in-flow block after <body> (html and body are
  // 640x360 in the fixture), so the document is 1000 wide and 360 + 2000 high.
  const spacer = document.createElement("div");
  spacer.setAttribute("id", "scroll-spacer");
  spacer.setAttribute("style", "width: 1000px; height: 2000px;");
  root.appendChild(spacer);
  // Overflow element: 200x100 padding box, 2px border, 600x500 content with an
  // item at content y = 300 (padding-top, so no margin collapsing).
  const scroller = document.createElement("div");
  scroller.setAttribute("id", "scroll-box");
  scroller.setAttribute(
    "style",
    "position: absolute; left: 400px; top: 20px; width: 200px; height: 100px; padding: 0; border: 2px solid black; overflow: auto; scrollbar-width: none;",
  );
  const content = document.createElement("div");
  content.setAttribute("style", "width: 600px; height: 200px; padding-top: 300px;");
  const item = document.createElement("div");
  item.setAttribute("id", "scroll-item");
  item.setAttribute("style", "width: 50px; height: 20px;");
  content.appendChild(item);
  scroller.appendChild(content);
  document.body.appendChild(scroller);

  try {
    for (const name of ["scroll", "scrollTo", "scrollBy"]) {
      check(typeof globalThis[name] === "function", `window.${name}`);
      check(typeof Element.prototype[name] === "function", `Element.prototype.${name}`);
    }
    check(typeof Element.prototype.scrollIntoView === "function", "Element.prototype.scrollIntoView");
    check(document.scrollingElement === root, "document.scrollingElement is the root element");
    check(item.parentElement === content && root.parentElement === null, "parentElement");

    // Viewport metrics.
    check(scrollX === 0 && scrollY === 0 && pageXOffset === 0 && pageYOffset === 0, `initial window scroll ${scrollX},${scrollY}`);
    check(root.clientWidth === 640 && root.clientHeight === 360, `root client size ${root.clientWidth}x${root.clientHeight}`);
    check(near(root.scrollWidth, 1000) && near(root.scrollHeight, 2360), `root scroll size ${root.scrollWidth}x${root.scrollHeight}`);

    const box0 = scroller.getBoundingClientRect();
    const item0 = item.getBoundingClientRect();
    check(near(box0.x, 400) && near(box0.y, 20), `scroller rect ${box0.x},${box0.y}`);
    check(near(item0.y - box0.y, 302) && near(item0.x - box0.x, 2), `item rect relative to scroller ${item0.x - box0.x},${item0.y - box0.y}`);

    // window.scrollTo with options (template's history restoration); smooth is
    // applied immediately.
    scrollTo({ top: 150, left: 40, behavior: "smooth" });
    check(scrollY === 150 && scrollX === 40 && pageYOffset === 150, `scrollTo options -> ${scrollX},${scrollY}`);
    check(root.scrollTop === 150 && root.scrollLeft === 40, "root scrollTop/Left follow the viewport");
    const box1 = scroller.getBoundingClientRect();
    check(near(box1.x, box0.x - 40) && near(box1.y, box0.y - 150), `rect after viewport scroll ${box1.x},${box1.y}`);
    scrollTo(10, 20);
    check(scrollX === 10 && scrollY === 20, `scrollTo(x, y) -> ${scrollX},${scrollY}`);
    scrollBy(5, 5);
    check(scrollX === 15 && scrollY === 25, `scrollBy(x, y) -> ${scrollX},${scrollY}`);
    scroll({ top: 99999 });
    check(near(scrollY, 2000) && scrollX === 15, `clamped to max, left kept -> ${scrollX},${scrollY}`);
    scrollTo({ left: -50 });
    check(scrollX === 0 && near(scrollY, 2000), `negative clamps to 0 -> ${scrollX},${scrollY}`);
    scrollTo({ top: NaN });
    check(scrollY === 0, `non-finite normalizes to 0 -> ${scrollY}`);
    root.scrollTop = 30;
    check(scrollY === 30, `root.scrollTop setter -> ${scrollY}`);
    scrollBy({ top: -10 });
    check(scrollY === 20, `scrollBy options -> ${scrollY}`);
    root.scrollTo({ top: 0, left: 0, behavior: "auto" });
    check(scrollX === 0 && scrollY === 0, `root.scrollTo resets -> ${scrollX},${scrollY}`);
    check(throwsTypeError(() => scrollTo({ top: 1, behavior: "fast" })), "invalid behavior throws TypeError");
    check(throwsTypeError(() => scrollTo(5)), "scrollTo(number) throws TypeError");
    check(scrollY === 0, "failed scrollTo calls do not move");

    // Overflow element metrics and offsets.
    check(scroller.clientWidth === 200 && scroller.clientHeight === 100, `scroller client ${scroller.clientWidth}x${scroller.clientHeight}`);
    check(scroller.clientLeft === 2 && scroller.clientTop === 2, `scroller clientLeft/Top ${scroller.clientLeft},${scroller.clientTop}`);
    check(near(scroller.scrollWidth, 600) && near(scroller.scrollHeight, 500), `scroller scroll size ${scroller.scrollWidth}x${scroller.scrollHeight}`);
    check(scroller.scrollTop === 0 && scroller.scrollLeft === 0, "scroller starts at 0,0");
    scroller.scrollTo({ top: 120, behavior: "smooth" });
    check(scroller.scrollTop === 120 && scroller.scrollLeft === 0, `element smooth scrollTo -> ${scroller.scrollLeft},${scroller.scrollTop}`);
    const box2 = scroller.getBoundingClientRect();
    const item2 = item.getBoundingClientRect();
    check(near(box2.y, box0.y), `scroller's own rect does not move with its content: ${box2.y}`);
    check(near(item2.y, item0.y - 120), `content rect moves by -scrollTop: ${item2.y}`);
    check(scrollY === 0, "element scroll does not scroll the viewport");
    scroller.scrollTo({ top: 5000 });
    check(near(scroller.scrollTop, 400), `element clamped to scrollHeight - clientHeight -> ${scroller.scrollTop}`);
    scroller.scrollLeft = 9999;
    check(near(scroller.scrollLeft, 400), `scrollLeft setter clamps -> ${scroller.scrollLeft}`);
    scroller.scrollBy({ left: -100, top: -100 });
    check(near(scroller.scrollLeft, 300) && near(scroller.scrollTop, 300), `element scrollBy -> ${scroller.scrollLeft},${scroller.scrollTop}`);
    scroller.scroll(0, 0);
    check(scroller.scrollTop === 0 && scroller.scrollLeft === 0, "element scroll(x, y)");
    const app = document.getElementById("app");
    app.scrollTop = 50;
    app.scrollTo({ top: 50, left: 50 });
    check(app.scrollTop === 0 && app.scrollLeft === 0, "overflow: visible elements do not scroll");
    check(scrollY === 0, "a non-scrolling element does not scroll the viewport");

    // Template centring: rect delta + scrollTop - clientHeight / 2.
    scroller.scrollTop = 100;
    const centre = item.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop - scroller.clientHeight / 2;
    check(near(centre, 252), `template centring target ${centre}`);
    scroller.scrollTo({ top: centre, behavior: "smooth" });
    check(near(scroller.scrollTop, 252), `template centring scrolls to its target -> ${scroller.scrollTop}`);

    // scrollIntoView: scroller first, then the viewport.
    const reset = () => {
      scroller.scrollTo(0, 0);
      scrollTo(0, 0);
    };
    reset();
    item.scrollIntoView();
    check(near(scroller.scrollTop, 300) && scroller.scrollLeft === 0, `scrollIntoView() block start -> ${scroller.scrollTop}`);
    check(near(scrollY, 22) && scrollX === 0, `scrollIntoView() then aligns the viewport -> ${scrollX},${scrollY}`);
    check(near(item.getBoundingClientRect().y, 0), `item at the viewport top: ${item.getBoundingClientRect().y}`);
    reset();
    item.scrollIntoView(false);
    check(near(scroller.scrollTop, 220) && scrollY === 0, `scrollIntoView(false) block end -> ${scroller.scrollTop},${scrollY}`);
    reset();
    item.scrollIntoView({ block: "center", behavior: "smooth" });
    check(near(scroller.scrollTop, 260), `block center -> ${scroller.scrollTop}`);
    reset();
    item.scrollIntoView({ block: "nearest" });
    check(near(scroller.scrollTop, 220), `block nearest below -> ${scroller.scrollTop}`);
    item.scrollIntoView({ block: "nearest" });
    check(near(scroller.scrollTop, 220), `block nearest when visible stays -> ${scroller.scrollTop}`);
    reset();
    check(throwsTypeError(() => item.scrollIntoView({ block: "middle" })), "invalid block throws TypeError");
    document.createElement("div").scrollIntoView();
    check(scrollY === 0 && scroller.scrollTop === 0, "detached scrollIntoView does nothing");
  } finally {
    scroller.scrollTo(0, 0);
    scrollTo(0, 0);
    document.body.removeChild(scroller);
    root.removeChild(spacer);
  }
  check(scrollX === 0 && scrollY === 0, `viewport restored -> ${scrollX},${scrollY}`);
  if (failures.length > 0) throw new Error(`scroll: ${failures.join(" | ")}`);
})();
