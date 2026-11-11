// Text controls on Blitz's own editor (stage `forms`). Checks the value API, then
// builds the controls main.rs types into through the window's input bridge
// (`forms_text_entry`: keys and an IME commit via Blitz's EventDriver);
// forms_check.js verifies the typed text and events and removes the controls.
(() => {
  "use strict";
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const errorName = (action) => {
    try {
      action();
    } catch (error) {
      return error?.name ?? String(error);
    }
    return null;
  };

  // Surface React DOM relies on.
  check(typeof InputEvent === "function" && InputEvent.prototype instanceof UIEvent, "InputEvent global");
  const constructed = new InputEvent("input", { data: "a", inputType: "insertText", bubbles: true });
  check(constructed.data === "a" && constructed.inputType === "insertText" && constructed.isComposing === false
    && constructed.bubbles && constructed.dataTransfer === null, "InputEvent init dictionary");
  check("oninput" in document && "oninput" in document.body && "oninput" in globalThis && document.body.oninput === null,
    "oninput handler attribute (React DOM detects the input event with 'oninput' in document)");
  const descriptor = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value");
  check(typeof descriptor?.get === "function" && typeof descriptor?.set === "function",
    "value accessor on HTMLInputElement.prototype (React's value tracker wraps it)");

  const range = document.createElement("input");
  range.type = "range";
  range.setAttribute("max", "10");
  range.setAttribute("step", "2");
  range.defaultValue = "4";
  range.value = "7";
  check(range.value === "8" && range.defaultValue === "4", "range value snaps independently of defaultValue");
  range.value = "100";
  check(range.value === "10", "range value clamps to max");

  // Default value and value of a detached input (no Blitz editor until a write).
  const detached = document.createElement("input");
  check(detached.value === "" && detached.defaultValue === "", "empty default value");
  detached.setAttribute("value", "def");
  check(detached.value === "def" && detached.defaultValue === "def", "default value from the attribute");
  detached.value = "a\r\nb";
  check(detached.value === "ab", `text values strip newlines: ${JSON.stringify(detached.value)}`);
  check(detached.getAttribute("value") === "def" && detached.defaultValue === "def", "a value write leaves the attribute");
  detached.value = null;
  check(detached.value === "", "null sets the empty string");
  // Known deviation (no dirty-value flag in Blitz): the attribute rewrites the editor.
  detached.defaultValue = "next";
  check(detached.getAttribute("value") === "next" && detached.value === "next", "defaultValue writes the attribute");
  check(errorName(() => { detached.value = Symbol("v"); }) === "TypeError", "DOMString conversion");
  const url = document.createElement("input");
  url.type = "URL";
  url.value = "  https://example.test/\n ";
  check(url.value === "https://example.test/", `url values are trimmed: ${JSON.stringify(url.value)}`);
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  check(checkbox.value === "on", "checkbox value defaults to on");
  checkbox.value = "yes";
  check(checkbox.getAttribute("value") === "yes" && checkbox.value === "yes", "checkbox value reflects the attribute");
  const date = document.createElement("input");
  date.type = "date";
  check(errorName(() => { date.value = "2026-11-11"; }) === "NotSupportedError",
    "writes to value-mode types nothing edits are refused, not faked");
  const detachedArea = document.createElement("textarea");
  detachedArea.textContent = "a\r\nb";
  check(detachedArea.value === "a\nb" && detachedArea.defaultValue === "a\r\nb",
    "textarea default value is its text, newlines normalized");
  detachedArea.value = "c\rd";
  check(detachedArea.value === "c\nd" && detachedArea.textContent === "a\r\nb", "textarea value write keeps the children");

  // Live controls. The name input mirrors React DOM's controlled input, reduced
  // from react-dom: the value is set while detached (initInput) and wrapped by a
  // tracker on the node (inputValueTracking); a root `input` listener reports a
  // change when the tracked value differs and restores the node to the state
  // (restoreControlledState), here rejecting digits.
  const container = document.createElement("div");
  container.setAttribute("style", "position: absolute; left: 0; top: 240px; width: 640px");
  const name = document.createElement("input");
  name.id = "form-name";
  let state = "Pe";
  name.value = state;
  name.defaultValue = state;
  const track = (node) => {
    const { get, set } = Object.getOwnPropertyDescriptor(node.constructor.prototype, "value");
    let current = "" + node.value;
    Object.defineProperty(node, "value", {
      configurable: true,
      get() {
        return get.call(this);
      },
      set(value) {
        current = "" + value;
        set.call(this, value);
      },
    });
    return { getValue: () => current, setValue: (value) => { current = "" + value; } };
  };
  const tracker = track(name);
  const log = { keydown: [], keyup: [], input: [], changes: [], oninput: 0 };
  name.addEventListener("keydown", (event) => {
    log.keydown.push(`${event.key}:${name.value}:${event.isTrusted}`);
    if (event.key === "x") event.preventDefault();
  });
  name.addEventListener("keyup", (event) => log.keyup.push(event.key));
  name.oninput = () => {
    log.oninput += 1;
  };
  const onRootInput = (event) => {
    if (event.target !== name) return;
    log.input.push({
      value: name.value,
      trusted: event.isTrusted,
      inputEvent: event instanceof InputEvent,
      bubbles: event.bubbles,
      cancelable: event.cancelable,
    });
    const next = name.value;
    if (next === tracker.getValue()) return;
    tracker.setValue(next);
    if (!/\d/.test(next)) {
      state = next;
      log.changes.push(next);
    }
    if (name.value !== state) name.value = state;
  };
  document.addEventListener("input", onRootInput);

  // `type="string"` is invalid, so the Text state (the template's name prompt).
  const typed = document.createElement("input");
  typed.id = "form-string";
  typed.setAttribute("type", "string");
  const area = document.createElement("textarea");
  area.id = "form-area";
  const inputs = { "form-string": [], "form-area": [] };
  for (const element of [typed, area]) {
    element.addEventListener("input", () => inputs[element.id].push(element.value));
  }
  for (const element of [name, typed, area]) container.appendChild(element);
  document.body.appendChild(container);
  check(typed.type === "text" && typed.value === "" && area.value === "", "empty live controls");
  name.focus();
  check(document.activeElement === name, "the name input takes focus");

  globalThis.formsTest = {
    log,
    inputs,
    tracker,
    state: () => state,
    focus(id) {
      document.getElementById(id).focus();
      if (document.activeElement?.id !== id) throw new Error(`#${id} did not take focus`);
    },
    cleanup() {
      document.removeEventListener("input", onRootInput);
      document.activeElement?.blur();
      document.body.removeChild(container);
    },
  };
})();
