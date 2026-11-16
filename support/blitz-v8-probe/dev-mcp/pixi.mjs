// Use the same application, stage and renderer globals recognized by PixiJS Devtools.
export const pixiStatus = `(() => {
  const app = globalThis.__PIXI_APP__;
  const stage = app?.stage ?? globalThis.__PIXI_STAGE__;
  const renderer = app?.renderer ?? globalThis.__PIXI_RENDERER__;
  const canvas = renderer?.canvas ?? renderer?.view;
  const detected = Boolean(app || stage || renderer || globalThis.PIXI ||
    globalThis.__PIXI__ || globalThis.__PIXI_DEVTOOLS__);
  let remaining = 32;
  function describe(node, depth) {
    remaining--;
    const children = Array.isArray(node.children) ? node.children : [];
    const result = {
      label: typeof node.label === "string" ? node.label.slice(0, 80) : null,
      type: node.constructor?.name?.slice(0, 80) ?? null,
      visible: typeof node.visible === "boolean" ? node.visible : null,
      child_count: children.length,
      children: [],
    };
    for (const child of children) {
      if (depth === 0 || remaining === 0) break;
      if (child && typeof child === "object") result.children.push(describe(child, depth - 1));
    }
    return result;
  }
  return {
    status: stage ? "scene_available" : detected ? "detected_without_scene" : "not_detected",
    renderer: renderer ? {
      backend: renderer.name ?? ({ 1: "webgl", 2: "webgpu", 4: "canvas" })[renderer.type] ?? null,
      resolution: renderer.resolution ?? null,
      canvas_width: canvas?.width ?? null,
      canvas_height: canvas?.height ?? null,
    } : null,
    scene: stage ? describe(stage, 3) : null,
  };
})()`;
