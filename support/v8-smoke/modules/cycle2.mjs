import { one } from "./cycle1.mjs";
export function two() { return one() + 1; }
