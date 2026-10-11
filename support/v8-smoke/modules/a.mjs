import { b, counter } from "./b.mjs";
export const a = "a" + b;
export function bump() { return counter.value++; }
export const metaUrl = import.meta.url;
