# rts-vs-electron (Electron)

App Electron mínima (BrowserWindow 1100x750) que abre `../app/index.html` — a mesma app React usada para comparar com o `rts` — para medir o custo de arranque/tamanho do runtime Electron em vez do da app em si.

Para construir: copie esta pasta e `../app/index.html` para fora do repo (ex.: `%TEMP%\rts-vs-electron\electron\`, com o `index.html` dentro de `electron\app\`, pois o packager só empacota ficheiros de dentro da pasta corrida), corra `npm install` lá dentro e depois `npx @electron/packager . rts-vs-electron --platform=win32 --arch=x64 --out=<destino> --overwrite`. Não corra `npm install`/`packager` dentro do repo — `node_modules` e `dist` não podem entrar no git.
