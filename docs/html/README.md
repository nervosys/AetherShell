# AetherShell HTML Documentation

This directory contains the complete HTML documentation site for AetherShell.

## Pages

- **index.html** - Home page with features, quick links, and getting started
- **quickstart.html** - Quick start guide for new users
- **tutorial.html** - Step-by-step interactive tutorial (10 lessons)
- **syntax.html** - Complete syntax reference with grammar specification
- **builtins.html** - All 72 built-in functions with examples
- **types.html** - Type system guide (Hindley-Milner type inference)
- **ai.html** - AI & agents guide (multi-modal, swarms, protocols)
- **syntax-kb.html** - Syntax Knowledge Base & AgenticBinary reference
- **examples.html** - Categorized code examples
- **api.html** - API reference for programmatic usage

## Styling

- **styles.css** - Comprehensive responsive CSS framework
  - Purple/gradient theme (#667eea primary, #764ba2 secondary)
  - Dark sidebar navigation (#1a202c)
  - Code syntax highlighting
  - Responsive design (mobile breakpoints)
  - Card grids, tables, alerts, badges

## Viewing

### Local Viewing
Simply open any HTML file in your browser:

```bash
# Windows
start docs/html/index.html

# Mac
open docs/html/index.html

# Linux
xdg-open docs/html/index.html
```

### Web Server (Optional)
For best experience, serve with a local web server:

```bash
# Python 3
cd docs/html
python -m http.server 8000

# Node.js (with http-server)
cd docs/html
npx http-server

# Then visit: http://localhost:8000
```

## Features

✅ Modern responsive design
✅ Fixed sidebar navigation
✅ Syntax highlighting for code
✅ Comprehensive examples
✅ Mobile-friendly layout
✅ Gradient themes and smooth animations
✅ Complete API reference
✅ Interactive tutorial with exercises

## Structure

```
docs/html/
├── index.html          # Home page
├── quickstart.html     # Quick start guide
├── tutorial.html       # Interactive tutorial
├── syntax.html         # Language syntax reference
├── builtins.html       # Built-in functions (all 72)
├── types.html          # Type system guide
├── ai.html             # AI & agents
├── syntax-kb.html      # Syntax KB & AgenticBinary
├── examples.html       # Code examples
├── api.html            # API reference
└── styles.css          # CSS framework
```

## Building for Production

To deploy the documentation site:

1. Copy entire `docs/html/` directory to web server
2. Ensure all `.html` and `.css` files are copied
3. Set appropriate MIME types (should be automatic)
4. No build step required - pure HTML/CSS

## Customization

### Theme Colors
Edit `styles.css` to change the color scheme:

```css
:root {
    --primary-color: #667eea;   /* Purple */
    --secondary-color: #764ba2; /* Dark purple */
    --text-color: #2d3748;      /* Dark gray */
    --bg-color: #f7fafc;        /* Light gray */
}
```

### Navigation
Add new pages by:
1. Creating new `.html` file
2. Adding navigation link in sidebar (all pages)
3. Updating card grids on home page

## Maintenance

When adding new features to AetherShell:
- Update relevant page (syntax, builtins, examples, etc.)
- Add examples to `examples.html`
- Update version number in sidebar (`<p class="version">`)
- Add to changelog if significant

## Credits

Built for AetherShell v1.0.0
Documentation created: 2025
Style: Modern gradient purple theme
Framework: Vanilla HTML5 + CSS3 (no dependencies)
