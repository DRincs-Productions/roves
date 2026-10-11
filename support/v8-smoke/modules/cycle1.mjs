import { two } from "./cycle2.mjs";
export function one() { return 1; }
export const sum = () => one() + two();
