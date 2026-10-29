const key = new URL(import.meta.url).searchParams.get("case");
const state = globalThis.rovesTlaCases[key];
state.executions++; state.entered();
await new Promise((resolve, reject) => {
  state.release = fail => fail ? reject(state.failure) : resolve();
});
state.completed = true;
export const value = 42;
