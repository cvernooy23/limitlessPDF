import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import "./lib/theme.svelte"; // initialize + apply theme before mount

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
