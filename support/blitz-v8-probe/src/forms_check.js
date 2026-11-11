// Results of the key/IME input main.rs sent in stage `forms` (see forms_test.js).
(() => {
  "use strict";
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const json = JSON.stringify;
  const { log, inputs, tracker } = formsTest;
  const name = document.getElementById("form-name");

  // Name input: "Pe" set while detached survived layout; "x" was canceled at
  // keydown (no default action, no input event); "1" was typed by Blitz and then
  // reverted by the controlled listener's value write, with the caret at the end.
  check(name.value === "Peter" && formsTest.state() === "Peter" && tracker.getValue() === "Peter",
    `typed name ${json(name.value)}, state ${json(formsTest.state())}`);
  check(name.getAttribute("value") === "Pe", "typing does not write the value attribute");
  check(json(log.keydown) === json([
    "t:Pe:true", "x:Pet:true", "e:Pet:true", "1:Pete:true", "r:Pete:true", "r:Peter:true",
    "Backspace:Peterr:true", "Enter:Peter:true",
  ]), `trusted keydown sequence before each default action: ${json(log.keydown)}`);
  check(log.keyup.join() === "t,x,e,1,r,r,Backspace,Enter", `keyup sequence: ${log.keyup.join()}`);
  check(json(log.input.map((entry) => entry.value)) === json(["Pet", "Pete", "Pete1", "Peter", "Peterr", "Peter"]),
    `input events carry Blitz's edited value: ${json(log.input)}`);
  check(log.input.every((entry) => entry.trusted && entry.inputEvent && entry.bubbles && !entry.cancelable),
    `input events are trusted, bubbling, non-cancelable InputEvents: ${json(log.input)}`);
  check(log.oninput === 6, `oninput handler ran ${log.oninput} times`);
  check(json(log.changes) === json(["Pet", "Pete", "Peter", "Peterr", "Peter"]),
    `controlled changes (digit rejected): ${json(log.changes)}`);

  // `type="string"`: two keys and an IME commit.
  const typed = document.getElementById("form-string");
  check(typed.value === "ok è" && json(inputs["form-string"]) === json(["o", "ok", "ok è"]),
    `type="string" input edited by Blitz: ${json(typed.value)} ${json(inputs["form-string"])}`);
  // Textarea: Enter inserts a line break.
  const area = document.getElementById("form-area");
  check(document.activeElement === area, "the textarea has focus");
  check(area.value === "a\nb" && json(inputs["form-area"]) === json(["a", "a\n", "a\nb"]),
    `textarea edited by Blitz: ${json(area.value)} ${json(inputs["form-area"])}`);
  // Programmatic writes on live controls go to the same editor.
  area.value = "x\r\ny";
  check(area.value === "x\ny", "textarea value write on a laid-out editor");

  formsTest.cleanup();
  check(document.activeElement === document.body && document.getElementById("form-name") === null,
    "forms test cleaned up");
  delete globalThis.formsTest;
})();
