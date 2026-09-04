// main.js — janela Electron minima para comparar tamanho/arranque com o rts.
//
// PORQUE contextIsolation por omissao (nao definido explicitamente aqui):
// a app React embutida (app/index.html) nao usa nenhuma API do processo
// principal (sem preload, sem ipcRenderer) — e so HTML+JS de pagina, o mesmo
// ficheiro que o `rts` abre. Ligar contextIsolation a mao seria simular uma
// integracao que esta comparacao nao precisa; o valor por omissao do
// Electron ja e o mais seguro e e o que uma app real usaria sem pedir nada.
//
// PORQUE sem devtools: abrir o DevTools muda o custo de memoria/arranque que
// estamos a medir — a comparacao e "janela pronta a mostrar a app", nao
// "janela + inspector".
//
// PORQUE app/index.html (nao ../app/index.html): o @electron/packager so
// inclui ficheiros de DENTRO da pasta onde corre (esta pasta, electron/).
// Um caminho relativo para fora dela (../app) fica no disco de
// desenvolvimento mas nao vai para o pacote final. O passo de build copia
// scripts/rts_vs_electron/app/index.html para uma copia SOMENTE na pasta
// temporaria de empacotamento (nunca no repo) em electron/app/index.html,
// para que o packager a apanhe.

const { app, BrowserWindow } = require('electron');
const path = require('path');

function createWindow() {
  const win = new BrowserWindow({
    width: 1100,
    height: 750,
    // webPreferences deliberadamente vazio: contextIsolation, sandbox e
    // nodeIntegration ficam nos valores por omissao do Electron.
  });

  win.loadFile(path.join(__dirname, 'app', 'index.html'));
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
