//! Markup language family integration tests
//!
//! Tests for HTML, CSS, SCSS, and Markdown - document and styling
//! markup languages.

#![allow(unused_imports)]
#![allow(clippy::duplicate_mod)]

#[path = "../common/mod.rs"]
mod common;
use common::{assertions::*, TestRepo};

// =============================================================================
// HTML TESTS
// =============================================================================

mod html_tests {
    use super::*;

    #[test]
    fn test_html_basic_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/index.html",
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>My Page</title>
    <link rel="stylesheet" href="styles.css">
</head>
<body>
    <header>
        <nav>
            <ul>
                <li><a href="/">Home</a></li>
                <li><a href="/about">About</a></li>
            </ul>
        </nav>
    </header>

    <main>
        <h1>Welcome</h1>
        <p>This is the main content.</p>
    </main>

    <footer>
        <p>&copy; 2024 My Company</p>
    </footer>

    <script src="app.js"></script>
</body>
</html>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/index.html", "-f", "json"]);
        assert!(output.unwrap().status.success(), "Should handle HTML file");
    }

    #[test]
    fn test_html_forms() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/form.html",
            r#"<!DOCTYPE html>
<html>
<body>
    <form action="/submit" method="POST" id="contact-form">
        <fieldset>
            <legend>Contact Information</legend>

            <label for="name">Name:</label>
            <input type="text" id="name" name="name" required>

            <label for="email">Email:</label>
            <input type="email" id="email" name="email" required>

            <label for="message">Message:</label>
            <textarea id="message" name="message" rows="5"></textarea>

            <label for="category">Category:</label>
            <select id="category" name="category">
                <option value="general">General</option>
                <option value="support">Support</option>
                <option value="sales">Sales</option>
            </select>

            <label>
                <input type="checkbox" name="subscribe"> Subscribe to newsletter
            </label>

            <button type="submit">Send</button>
            <button type="reset">Clear</button>
        </fieldset>
    </form>
</body>
</html>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/form.html", "-f", "json"]);
        assert!(output.unwrap().status.success(), "Should handle HTML forms");
    }

    #[test]
    fn test_html_semantic() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/article.html",
            r#"<!DOCTYPE html>
<html>
<body>
    <article itemscope itemtype="https://schema.org/Article">
        <header>
            <h1 itemprop="headline">Article Title</h1>
            <time itemprop="datePublished" datetime="2024-01-15">
                January 15, 2024
            </time>
            <address itemprop="author">
                By <a rel="author" href="/authors/john">John Doe</a>
            </address>
        </header>

        <figure>
            <img src="hero.jpg" alt="Hero image" itemprop="image">
            <figcaption>A descriptive caption</figcaption>
        </figure>

        <section itemprop="articleBody">
            <p>First paragraph of the article.</p>

            <aside>
                <h3>Related Information</h3>
                <p>Some related content here.</p>
            </aside>

            <p>More content continues here.</p>
        </section>

        <footer>
            <details>
                <summary>Sources</summary>
                <ul>
                    <li>Source 1</li>
                    <li>Source 2</li>
                </ul>
            </details>
        </footer>
    </article>
</body>
</html>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/article.html", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle semantic HTML"
        );
    }

    #[test]
    fn test_html_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.html", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/empty.html", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty HTML file"
        );
    }

    #[test]
    fn test_html_with_embedded() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/embedded.html",
            r#"<!DOCTYPE html>
<html>
<head>
    <style>
        body {
            font-family: Arial, sans-serif;
            margin: 0;
            padding: 20px;
        }

        .container {
            max-width: 1200px;
            margin: 0 auto;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>Page with Embedded Content</h1>
    </div>

    <script>
        document.addEventListener('DOMContentLoaded', function() {
            console.log('Page loaded');

            const container = document.querySelector('.container');
            container.addEventListener('click', function(e) {
                console.log('Container clicked');
            });
        });
    </script>
</body>
</html>
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/embedded.html", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle HTML with embedded CSS/JS"
        );
    }
}

// =============================================================================
// CSS TESTS
// =============================================================================

mod css_tests {
    use super::*;

    #[test]
    fn test_css_basic_extraction() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/styles.css",
            r#"/* Base styles */
* {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
}

body {
    font-family: 'Helvetica Neue', Arial, sans-serif;
    font-size: 16px;
    line-height: 1.6;
    color: #333;
}

/* Layout */
.container {
    max-width: 1200px;
    margin: 0 auto;
    padding: 0 20px;
}

/* Typography */
h1, h2, h3 {
    margin-bottom: 1rem;
}

h1 { font-size: 2.5rem; }
h2 { font-size: 2rem; }
h3 { font-size: 1.5rem; }

/* Links */
a {
    color: #0066cc;
    text-decoration: none;
}

a:hover {
    text-decoration: underline;
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/styles.css", "-f", "json"]);
        assert!(output.unwrap().status.success(), "Should handle CSS file");
    }

    #[test]
    fn test_css_modern_features() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/modern.css",
            r#"/* CSS Custom Properties */
:root {
    --primary-color: #3498db;
    --secondary-color: #2ecc71;
    --font-size-base: 16px;
    --spacing-unit: 8px;
}

/* Flexbox */
.flex-container {
    display: flex;
    flex-direction: row;
    justify-content: space-between;
    align-items: center;
    gap: calc(var(--spacing-unit) * 2);
}

/* Grid */
.grid-container {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    grid-gap: 20px;
}

/* Container queries */
@container (min-width: 400px) {
    .card {
        display: grid;
        grid-template-columns: 1fr 2fr;
    }
}

/* Media queries */
@media (max-width: 768px) {
    .flex-container {
        flex-direction: column;
    }
}

/* Nesting (CSS Nesting) */
.nav {
    & ul {
        list-style: none;
    }

    & a {
        color: var(--primary-color);

        &:hover {
            color: var(--secondary-color);
        }
    }
}

/* Logical properties */
.box {
    margin-inline: auto;
    padding-block: 1rem;
    border-inline-start: 3px solid var(--primary-color);
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/modern.css", "-f", "json"]);
        assert!(output.unwrap().status.success(), "Should handle modern CSS");
    }

    #[test]
    fn test_css_animations() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/animations.css",
            r#"/* Keyframes */
@keyframes fadeIn {
    from {
        opacity: 0;
        transform: translateY(-20px);
    }
    to {
        opacity: 1;
        transform: translateY(0);
    }
}

@keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
}

@keyframes pulse {
    0%, 100% { transform: scale(1); }
    50% { transform: scale(1.05); }
}

/* Animation usage */
.fade-in {
    animation: fadeIn 0.3s ease-out forwards;
}

.spinner {
    animation: spin 1s linear infinite;
}

.pulse {
    animation: pulse 2s ease-in-out infinite;
}

/* Transitions */
.button {
    background-color: #3498db;
    transition: background-color 0.3s ease,
                transform 0.2s ease,
                box-shadow 0.2s ease;
}

.button:hover {
    background-color: #2980b9;
    transform: translateY(-2px);
    box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/animations.css", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle CSS animations"
        );
    }

    #[test]
    fn test_css_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.css", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/empty.css", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty CSS file"
        );
    }
}

// =============================================================================
// SCSS TESTS
// =============================================================================

mod scss_tests {
    use super::*;

    #[test]
    fn test_scss_variables() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/styles.scss",
            r#"// Variables
$primary-color: #3498db;
$secondary-color: #2ecc71;
$font-stack: 'Helvetica Neue', Arial, sans-serif;
$base-font-size: 16px;
$spacing: 8px;

// Maps
$breakpoints: (
    'sm': 576px,
    'md': 768px,
    'lg': 992px,
    'xl': 1200px
);

$colors: (
    'primary': $primary-color,
    'secondary': $secondary-color,
    'success': #27ae60,
    'danger': #e74c3c,
    'warning': #f39c12
);

// Usage
body {
    font-family: $font-stack;
    font-size: $base-font-size;
    color: $primary-color;
}

.container {
    padding: $spacing * 2;
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/styles.scss", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle SCSS variables"
        );
    }

    #[test]
    fn test_scss_nesting() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/nesting.scss",
            r#"$primary-color: #3498db;

// Nesting
.nav {
    background: #333;

    ul {
        list-style: none;
        margin: 0;
        padding: 0;

        li {
            display: inline-block;

            a {
                display: block;
                padding: 10px 15px;
                color: white;

                &:hover {
                    background: lighten(#333, 10%);
                }

                &.active {
                    background: $primary-color;
                }
            }
        }
    }
}

// Parent selector
.btn {
    padding: 10px 20px;

    &-primary {
        background: $primary-color;
    }

    &-secondary {
        background: #2ecc71;
    }

    &--large {
        padding: 15px 30px;
        font-size: 1.2em;
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/nesting.scss", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle SCSS nesting"
        );
    }

    #[test]
    fn test_scss_mixins() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/mixins.scss",
            r#"$primary-color: #3498db;
$breakpoints: (
    'sm': 576px,
    'md': 768px,
    'lg': 992px
);

// Simple mixin
@mixin flex-center {
    display: flex;
    justify-content: center;
    align-items: center;
}

// Mixin with parameters
@mixin button($bg-color, $text-color: white) {
    background-color: $bg-color;
    color: $text-color;
    padding: 10px 20px;
    border: none;
    border-radius: 4px;
    cursor: pointer;

    &:hover {
        background-color: darken($bg-color, 10%);
    }
}

// Mixin with content block
@mixin media($breakpoint) {
    @if map-has-key($breakpoints, $breakpoint) {
        @media (min-width: map-get($breakpoints, $breakpoint)) {
            @content;
        }
    }
}

// Usage
.centered {
    @include flex-center;
}

.btn-primary {
    @include button($primary-color);
}

.btn-danger {
    @include button(#e74c3c, white);
}

.responsive-container {
    width: 100%;

    @include media('md') {
        width: 720px;
    }

    @include media('lg') {
        width: 960px;
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/mixins.scss", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle SCSS mixins"
        );
    }

    #[test]
    fn test_scss_functions() {
        let repo = TestRepo::new();
        repo.add_file(
            "src/functions.scss",
            r#"$base-spacing: 8px;
$colors: (
    'primary': #3498db,
    'secondary': #2ecc71,
    'success': #27ae60,
    'danger': #e74c3c,
    'warning': #f39c12
);

// Custom functions
@function rem($pixels) {
    @return ($pixels / 16px) * 1rem;
}

@function contrast-color($color) {
    @if lightness($color) > 50% {
        @return #000;
    } @else {
        @return #fff;
    }
}

@function spacing($multiplier: 1) {
    @return $base-spacing * $multiplier;
}

// Usage
.element {
    font-size: rem(18px);
    padding: spacing(2);
    margin-bottom: rem(24px);
}

@each $name, $color in $colors {
    .bg-#{$name} {
        background-color: $color;
        color: contrast-color($color);
    }
}

// Control flow
@for $i from 1 through 5 {
    .col-#{$i} {
        width: percentage($i / 12);
    }
}
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/functions.scss", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle SCSS functions"
        );
    }

    #[test]
    fn test_scss_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("src/empty.scss", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "src/empty.scss", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty SCSS file"
        );
    }
}

// =============================================================================
// MARKDOWN TESTS
// =============================================================================

mod markdown_tests {
    use super::*;

    #[test]
    fn test_markdown_basic() {
        let repo = TestRepo::new();
        repo.add_file(
            "docs/README.md",
            r#"# Project Title

A brief description of the project.

## Installation

```bash
npm install my-package
```

## Usage

```javascript
const myPackage = require('my-package');
myPackage.doSomething();
```

## Features

- Feature 1
- Feature 2
- Feature 3

## API Reference

### `doSomething(options)`

Does something useful.

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| options | Object | Configuration options |
| options.verbose | boolean | Enable verbose output |

**Returns:** `Promise<Result>`

## License

MIT
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docs/README.md", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle Markdown file"
        );
    }

    #[test]
    fn test_markdown_extended() {
        let repo = TestRepo::new();
        repo.add_file(
            "docs/guide.md",
            r#"# Extended Markdown Features

## Task Lists

- [x] Completed task
- [ ] Incomplete task
- [ ] Another task

## Tables

| Header 1 | Header 2 | Header 3 |
|:---------|:--------:|---------:|
| Left     | Center   | Right    |
| aligned  | aligned  | aligned  |

## Footnotes

Here's a sentence with a footnote.[^1]

[^1]: This is the footnote content.

## Definitions

term
: definition of the term

## Abbreviations

The HTML specification is maintained by the W3C.

*[HTML]: Hyper Text Markup Language
*[W3C]: World Wide Web Consortium

## Blockquotes

> This is a blockquote.
>
> It can span multiple paragraphs.
>
> > Nested blockquotes are also possible.

## Horizontal Rule

---

## Links and Images

[Link text](https://example.com "Optional title")

![Alt text](image.png "Image title")

Reference-style [link][ref].

[ref]: https://example.com "Reference link"

## Emphasis

*italic* or _italic_
**bold** or __bold__
***bold italic*** or ___bold italic___
~~strikethrough~~
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docs/guide.md", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle extended Markdown"
        );
    }

    #[test]
    fn test_markdown_code_blocks() {
        let repo = TestRepo::new();
        repo.add_file(
            "docs/examples.md",
            r#"# Code Examples

## JavaScript

```javascript
function greet(name) {
    return `Hello, ${name}!`;
}

console.log(greet('World'));
```

## Python

```python
def greet(name):
    return f"Hello, {name}!"

print(greet("World"))
```

## Rust

```rust
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

fn main() {
    println!("{}", greet("World"));
}
```

## Inline Code

Use `npm install` to install dependencies.

The `main()` function is the entry point.

## Diff

```diff
- const old = "old value";
+ const new = "new value";
```

## JSON with Highlighting

```json
{
  "name": "example",
  "version": "1.0.0",
  "dependencies": {
    "package": "^1.0.0"
  }
}
```
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docs/examples.md", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle Markdown with code blocks"
        );
    }

    #[test]
    fn test_markdown_empty_file() {
        let repo = TestRepo::new();
        repo.add_file("docs/empty.md", "");
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docs/empty.md", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle empty Markdown file"
        );
    }

    #[test]
    fn test_markdown_frontmatter() {
        let repo = TestRepo::new();
        repo.add_file(
            "docs/post.md",
            r#"---
title: My Blog Post
date: 2024-01-15
author: John Doe
tags:
  - rust
  - programming
draft: false
---

# My Blog Post

This is the content of the blog post.

It supports all standard Markdown features.
"#,
        );
        repo.generate_index().unwrap();

        let output = repo.run_cli(&["analyze", "docs/post.md", "-f", "json"]);
        assert!(
            output.unwrap().status.success(),
            "Should handle Markdown with frontmatter"
        );
    }
}
