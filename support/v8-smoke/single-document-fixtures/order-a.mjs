import { value } from "./value.mjs";
if (!document.getElementById("game-root")) throw new Error("module ran before the body was parsed");
order.push("a:" + value);
await new Promise(resolve => document.addEventListener("DOMContentLoaded", resolve, { once: true }));
order.push("a:after-domcontentloaded");
