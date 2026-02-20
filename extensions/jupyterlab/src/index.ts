import {
  JupyterFrontEnd,
  JupyterFrontEndPlugin,
} from '@jupyterlab/application';

/**
 * AetherShell JupyterLab extension
 *
 * Provides:
 * - AetherShell kernel proxy (connects to `ae agent serve`)
 * - Syntax highlighting for .ae files
 * - Code completion via Agent API
 */
const plugin: JupyterFrontEndPlugin<void> = {
  id: 'jupyterlab-aethershell:plugin',
  autoStart: true,
  activate: (app: JupyterFrontEnd) => {
    console.log('JupyterLab AetherShell extension activated');

    // Register .ae file type
    app.docRegistry.addFileType({
      name: 'aethershell',
      displayName: 'AetherShell Script',
      extensions: ['.ae'],
      mimeTypes: ['text/x-aethershell'],
      icon: undefined,
    });
  },
};

export default plugin;
