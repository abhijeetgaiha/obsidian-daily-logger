import { api } from "./api";
import { mountJournal } from "./ui";

const root = document.querySelector<HTMLElement>("#app");
if (!root) throw new Error("Missing application root.");
mountJournal(root, api);
