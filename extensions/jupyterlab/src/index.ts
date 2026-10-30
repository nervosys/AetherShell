import {
  JupyterFrontEnd,
  JupyterFrontEndPlugin,
} from '@jupyterlab/application';

/**
 * IronShell JupyterLab extension
 *
 * Provides:
 * - IronShell kernel proxy (connects to `ish agent serve`)
 * - Syntax highlighting for .ish files
 * - Code completion via Agent API
 */
const plugin: JupyterFrontEndPlugin<void> = {
  id: 'jupyterlab-ironshell:plugin',
  autoStart: true,
  activate: (app: JupyterFrontEnd) => {
    console.log('JupyterLab IronShell extension activated');

    // Register .ish file type
    app.docRegistry.addFileType({
      name: 'ironshell',
      displayName: 'IronShell Script',
      extensions: ['.ish'],
      mimeTypes: ['text/x-ironshell'],
      icon: undefined,
    });
  },
};

export default plugin;
