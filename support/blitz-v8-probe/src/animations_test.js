// Element.getAnimations over Stylo's CSS animations/transitions (dom.js,
// op_blitz_get_animations). Phases are driven from Rust (stage css_animations)
// with real frames in between, so Blitz's style flushes advance the native state
// on the page timeline. Covers: a 100 ms transition triggered by a class change,
// an infinite keyframes animation on a descendant (`subtree`, stable identity
// across iterations), a finite forwards animation (finished settlement, restart
// = new object), cancellation by removing the class and by removing the
// element (AbortError), and the visual-novel-template's Base UI callers
// (getFiniteAnimations, ScrollAreaViewport, useAnimationsFinished) de-minified
// from @base-ui/react 1.9.0. Removes its elements in the last phase.
(() => {
  "use strict";
  const result = { failures: [], checks: 0, phase: 0 };
  globalThis.animationsResult = result;
  const check = (condition, message) => {
    result.checks++;
    if (!condition) result.failures.push(`phase ${result.phase}: ${message}`);
  };
  const throwsTypeError = (action) => {
    try {
      action();
    } catch (error) {
      return error instanceof TypeError;
    }
    return false;
  };
  const isAbort = (error) => error instanceof DOMException && error.name === "AbortError";

  // Base UI utils/getFiniteAnimations (template bundle, minified names kept).
  function Os(e, t) {
    return e.getAnimations(t).filter((e) => {
      let t = e.effect?.getTiming();
      return t?.duration !== 1 / 0 && t?.iterations !== 1 / 0;
    });
  }
  // Base UI internals/useAnimationsFinished's `l` loop (flushSync and the abort
  // signal reduced to a direct call); started through requestAnimationFrame
  // like its `frame.request(l)`.
  function animationsFinished(element, done) {
    if (typeof element.getAnimations !== "function") {
      done("no-getAnimations");
      return;
    }
    function l() {
      Promise.all(Os(element).map((e) => e.finished.then(() => {}))).then(
        () => done("finished"),
        () => (Os(element).some((e) => e.pending || e.playState !== "finished") ? l() : done("settled")),
      );
    }
    requestAnimationFrame(l);
  }

  const style = document.createElement("style");
  style.textContent = `
    @keyframes roves-spin { from { opacity: 0.2 } to { opacity: 1 } }
    @keyframes roves-pulse { from { width: 10px } to { width: 20px } }
    @keyframes roves-hold { from { height: 10px } to { height: 20px } }
    #anim-parent { opacity: 1; transition: opacity 100ms linear; }
    #anim-parent.faded { opacity: 0.5; }
    #anim-child { height: 5px; animation: roves-spin 150ms linear infinite; }
    #anim-finite { height: 5px; width: 10px; }
    #anim-finite.run { animation: roves-pulse 150ms linear 1 forwards; }
    #anim-cancel { width: 5px; }
    #anim-cancel.run { animation: roves-hold 10s linear; }
    #anim-popup { opacity: 0; transition: opacity 120ms linear; }
    #anim-popup.open { opacity: 1; }
  `;
  document.head.appendChild(style);
  const box = (id, parent = document.body) => {
    const element = document.createElement("div");
    element.setAttribute("id", id);
    parent.appendChild(element);
    return element;
  };
  const parent = box("anim-parent");
  const child = box("anim-child", parent);
  const finite = box("anim-finite");
  const cancel = box("anim-cancel");
  const popup = box("anim-popup");
  const state = {};

  globalThis.animationsPhase = (phase) => {
    result.phase = phase;
    if (phase === 1) {
      // Before-change styles: the class changes below then start transitions.
      check(getComputedStyle(parent).opacity === "1", "parent starts opaque");
      check(getComputedStyle(popup).opacity === "0", "popup starts transparent");
      check(Array.isArray(finite.getAnimations()) && finite.getAnimations().length === 0, "no animation before the class");
      parent.classList.add("faded");
      popup.classList.add("open");
      finite.classList.add("run");
      cancel.classList.add("run");

      const own = parent.getAnimations();
      check(own.length === 1, `parent has one transition: ${own.length}`);
      const transition = own[0];
      state.transition = transition;
      check(transition?.constructor?.name === "CSSTransition", "transition is a CSSTransition");
      check(transition?.transitionProperty === "opacity", `transitionProperty: ${transition?.transitionProperty}`);
      check(transition?.id === "", "id is empty");
      check(transition?.effect?.constructor?.name === "KeyframeEffect", "effect is a KeyframeEffect");
      check(transition?.effect?.target === parent, "effect target is the element");
      check(transition?.effect?.pseudoElement === null, "no pseudo-element");
      const timing = transition?.effect?.getTiming() ?? {};
      check(timing.duration === 100 && timing.delay === 0 && timing.iterations === 1, `transition timing ${JSON.stringify(timing)}`);
      check(timing.fill === "backwards" && timing.direction === "normal", `transition fill/direction ${JSON.stringify(timing)}`);
      check(transition?.pending === true && transition?.playState === "running", `new transition pending/running: ${transition?.pending} ${transition?.playState}`);
      check(transition?.startTime === null, "pending transition has no start time");
      check(transition?.finished instanceof Promise, "finished is a promise");
      check(parent.getAnimations()[0] === transition, "repeated call returns the same object");
      check(parent.getAnimations()[0]?.finished === transition?.finished, "repeated call returns the same finished promise");
      transition?.finished.then((value) => { state.transitionResolved = value === transition; });

      const subtree = parent.getAnimations({ subtree: true });
      check(subtree.length === 2, `subtree has the transition and the child animation: ${subtree.length}`);
      check(subtree[0] === transition, "transitions come first, same object");
      const infinite = subtree[1];
      state.infinite = infinite;
      check(infinite?.constructor?.name === "CSSAnimation", "child animation is a CSSAnimation");
      check(infinite?.animationName === "roves-spin", `animationName: ${infinite?.animationName}`);
      check(infinite?.effect?.target === child, "child animation targets the child");
      const infiniteTiming = infinite?.effect?.getTiming() ?? {};
      check(infiniteTiming.iterations === Infinity && infiniteTiming.duration === 150, `infinite timing ${JSON.stringify(infiniteTiming)}`);
      check(infiniteTiming.fill === "none" && infiniteTiming.direction === "normal", `infinite fill/direction ${JSON.stringify(infiniteTiming)}`);
      check(child.getAnimations()[0] === infinite, "same object through the child itself");
      check(parent.getAnimations(null).length === 1 && parent.getAnimations({}).length === 1, "null/{} options exclude the subtree");
      infinite?.finished.then(
        () => { state.infinite_settled = "resolved"; },
        (error) => { state.infinite_settled = isAbort(error) ? "aborted" : `wrong: ${error}`; },
      );

      // ScrollAreaViewport: finite subtree animations, then allSettled.
      const finiteSubtree = Os(parent, { subtree: !0 });
      check(finiteSubtree.length === 1 && finiteSubtree[0] === transition, "getFiniteAnimations drops the infinite animation");
      finiteSubtree.length !== 0
        && Promise.allSettled(finiteSubtree.map((e) => e.finished.then(() => {}))).then(() => { state.scrollAreaSettled = true; })
          .catch(() => {});

      const finiteAnimation = finite.getAnimations()[0];
      state.finite = finiteAnimation;
      const finiteTiming = finiteAnimation?.effect?.getTiming() ?? {};
      check(finiteAnimation?.animationName === "roves-pulse", "finite animation name");
      check(finiteTiming.duration === 150 && finiteTiming.iterations === 1 && finiteTiming.fill === "forwards", `finite timing ${JSON.stringify(finiteTiming)}`);
      finiteAnimation?.finished.then((value) => { state.finiteResolved = value === finiteAnimation; });

      const held = cancel.getAnimations()[0];
      state.cancel = held;
      check(held?.animationName === "roves-hold", "cancel test animation exists");
      held?.finished.then(
        () => { state.cancel_settled = "resolved"; },
        (error) => { state.cancel_settled = isAbort(error) ? "aborted" : `wrong: ${error}`; },
      );

      animationsFinished(popup, (how) => { state.popup = how; });

      check(throwsTypeError(() => Element.prototype.getAnimations.call({})), "getAnimations on a non-element throws");
      check(throwsTypeError(() => parent.getAnimations(5)), "non-dictionary options throw");
      check(throwsTypeError(() => new transition.constructor()), "CSSTransition is not constructible");
      check(throwsTypeError(() => Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Object.getPrototypeOf(transition)), "playState").get.call({})), "playState getter checks its receiver");
      check(typeof globalThis.Animation === "undefined" && typeof globalThis.KeyframeEffect === "undefined", "no constructible Web Animations globals");
    } else if (phase === 2) {
      // About 330 ms of frames later: the 100/120/150 ms animations finished.
      const transition = state.transition;
      check(transition?.playState === "finished" && transition?.pending === false, `transition finished: ${transition?.playState}`);
      check(state.transitionResolved === true, "transition finished resolved with the animation");
      check(state.scrollAreaSettled === true, "ScrollArea allSettled ran");
      check(state.popup === "finished", `useAnimationsFinished completed: ${state.popup}`);
      check(state.finiteResolved === true, "finite animation finished resolved with the animation");
      check(parent.getAnimations().length === 0, "a finished transition is no longer relevant (real empty list)");
      const finiteNow = finite.getAnimations();
      check(finiteNow.length === 1 && finiteNow[0] === state.finite, "forwards-filled finished animation stays, same object");
      check(state.finite?.playState === "finished" && state.finite?.currentTime === 150, `finite end state ${state.finite?.playState} ${state.finite?.currentTime}`);
      const infinite = state.infinite;
      check(child.getAnimations()[0] === infinite, "infinite animation keeps its identity across iterations");
      check(infinite?.playState === "running" && infinite?.pending === false, `infinite running: ${infinite?.playState}`);
      check(typeof infinite?.startTime === "number" && infinite?.currentTime > 300, `infinite times ${infinite?.startTime} ${infinite?.currentTime}`);
      check(state.cancel?.playState === "running", "long animation still running");
      check(state.cancel_settled === undefined, "running animation's finished is unsettled");
      cancel.classList.remove("run");
      // Restart: removing the name (flushed) and adding it again is a new animation.
      finite.classList.remove("run");
      getComputedStyle(finite).width;
      finite.classList.add("run");
      const restarted = finite.getAnimations()[0];
      state.restarted = restarted;
      check(restarted !== undefined && restarted !== state.finite, "a restarted animation is a new object");
    } else if (phase === 3) {
      check(state.cancel_settled === "aborted", `class removal rejects finished with AbortError: ${state.cancel_settled}`);
      check(state.cancel?.playState === "idle", `canceled animation is idle: ${state.cancel?.playState}`);
      check(cancel.getAnimations().length === 0, "canceled animation is gone");
      check(state.finite?.playState === "idle", "the replaced animation is idle");
      check(finite.getAnimations()[0] === state.restarted, "the restarted animation keeps its identity");
      parent.removeChild(child);
    } else if (phase === 4) {
      check(state.infinite_settled === "aborted", `element removal rejects finished with AbortError: ${state.infinite_settled}`);
      check(state.infinite?.playState === "idle", "removed element's animation is idle");
      for (const element of [parent, finite, cancel, popup, style]) element.remove();
      result.done = true;
    }
  };
})();
