import { WebUI } from "@webui-dev/bun-webui";

const myWindow = new WebUI();

myWindow.show("<html><head><title>App</title></head><body style='font-family:sans-serif;background:#1a1b26;color:#c0caf5;'><h1>Hello depuis Bun & WebUI</h1></body></html>");

await WebUI.wait();