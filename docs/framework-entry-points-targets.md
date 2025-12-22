# Framework Entry Point Targets

This list captures candidate framework/library entry points that should surface as *non-dead* in static call graph analysis.
Grouped by **language → library/framework → type → notes**.

Legend:
- Completed: ✅ implemented in current detectors or boilerplate logic
- Planned/Missing: ☐ not yet implemented
- Structural-only: ◻ parsing only (no semantic entry-point logic yet)

## JavaScript

### Core JS (.js/.mjs/.cjs)
- Completed: ✅ (symbols + imports)
- Type: function/class exports, module entry
- Notes: generic JS module entry points

### React
- Completed: ✅
- Type: root component, component exports, hooks, context providers
- Notes: `createRoot`/`ReactDOM.render`, `useContext`, `useReducer` entry flows

### Next.js
- Completed: ✅ (file patterns + data fetching detection)
- Type: App Router files (`/app/**/page.tsx`, `layout.tsx`, `route.ts`), Pages Router (`/pages/**`), API routes, middleware
- Notes: data fetching (`getServerSideProps`, `getStaticProps`, `getStaticPaths`, `getInitialProps`)

### Express
- Completed: ✅
- Type: `app.METHOD`, `router.METHOD`, middleware chains
- Notes: `app.use`, `router.use`

### Angular
- Completed: ✅ (decorator detection)
- Type: `@Component`, `@NgModule`, `@Injectable`
- Notes: dependency injection, module wiring

### Vue
- Completed: ✅ (SFC parsing)
- Type: `.vue` SFC, component exports
- Notes: Composition API

### NestJS
- Completed: ✅ (framework detection)
- Type: `@Controller`, `@Module`, `@Injectable`, `bootstrap`
- Notes: DI-injected providers as implicit callees

### Koa
- Completed: ☐
- Type: `router.METHOD`, `app.use`
- Notes: middleware stack, composed flows

### Fastify
- Completed: ☐
- Type: `fastify.METHOD`, hooks, plugins
- Notes: `addHook`, `register`

### Hapi
- Completed: ☐
- Type: `server.route`, lifecycle methods
- Notes: `onPreHandler`, `onPostHandler`

### Sails / Adonis
- Completed: ☐
- Type: route/controller actions, policy hooks
- Notes: framework-provided bootstraps

### Remix
- Completed: ☐
- Type: route modules, `loader`, `action`, `default` export
- Notes: file-based routing

### Astro
- Completed: ☐
- Type: route files, `getStaticPaths`, API routes
- Notes: `.astro` and SSR endpoints

### SvelteKit
- Completed: ☐
- Type: `+page`, `+layout`, `+server` files
- Notes: `load`, actions, endpoints

### Nuxt
- Completed: ☐
- Type: `pages/`, `server/api/`, plugins
- Notes: conventions for modules and middleware

### Vercel / Netlify / AWS Lambda
- Completed: ☐
- Type: handler exports
- Notes: `handler`, `default` export patterns

### Cloudflare Workers
- Completed: ☐
- Type: `fetch` handler, `scheduled` handler
- Notes: service worker style entry points

### Socket.io / ws
- Completed: ☐
- Type: event handlers and connection hooks
- Notes: `io.on('connection', ...)`

### GraphQL (Apollo / Yoga / Helix)
- Completed: ☐
- Type: resolver map, schema definitions
- Notes: resolvers are framework-invoked

### Tooling (Vite / Webpack / Rollup / Babel)
- Completed: ☐
- Type: config entry + plugin hooks
- Notes: `defineConfig`, `plugins` with hooks

### CLI frameworks (Commander / Yargs / Oclif)
- Completed: ☐
- Type: command registration and handlers
- Notes: `program.command` chains


## TypeScript

### Core TS (.ts/.mts/.cts)
- Completed: ✅ (symbols + types)
- Type: exports, interfaces, enums, decorators
- Notes: TS-specific symbol extraction


## TSX

### Core TSX (.tsx)
- Completed: ✅ (JSX + React detection)
- Type: component exports, hooks
- Notes: React patterns on TSX


## JSX

### Core JSX (.jsx)
- Completed: ✅ (JSX + React detection)
- Type: component exports, hooks
- Notes: React patterns on JSX


## Rust

### Core Rust (.rs)
- Completed: ✅
- Type: functions, structs, traits, enums
- Notes: `pub` visibility and module paths

### Actix-web / Axum / Rocket / Warp
- Completed: ☐
- Type: route macros, handlers
- Notes: `#[get]`, `Router::route`

### Tonic (gRPC)
- Completed: ☐
- Type: service trait implementations
- Notes: generated server interfaces

### Bevy
- Completed: ☐
- Type: app entry + system registration
- Notes: `App::new().add_system`

### Godot (Rust)
- Completed: ☐
- Type: node lifecycle hooks
- Notes: `#[gdextension]`


## Python

### Core Python (.py/.pyi)
- Completed: ✅
- Type: functions, classes, decorators
- Notes: underscore-private convention

### Django
- Completed: ☐
- Type: views, URL patterns, management commands
- Notes: `urls.py`, `manage.py` commands

### Flask
- Completed: ☐
- Type: `@app.route` handlers
- Notes: blueprints

### FastAPI / Starlette
- Completed: ☐
- Type: `@app.get/post` handlers
- Notes: dependency injection

### Celery / RQ
- Completed: ☐
- Type: task decorators
- Notes: `@shared_task`, `@celery.task`

### Click / Typer
- Completed: ☐
- Type: CLI commands
- Notes: `@app.command`

### Airflow
- Completed: ☐
- Type: DAG definitions
- Notes: `DAG(...)` with tasks


## Go

### Core Go (.go)
- Completed: ✅
- Type: functions, methods, structs
- Notes: uppercase export convention

### net/http (stdlib)
- Completed: ☐
- Type: `http.HandleFunc`, `ServeMux`, `ListenAndServe`
- Notes: direct handler registration

### Gin / Echo / Fiber / Chi
- Completed: ☐
- Type: route registration
- Notes: `router.GET/POST`, middleware

### gRPC
- Completed: ☐
- Type: service impls, method handlers
- Notes: generated interfaces

### Cobra
- Completed: ☐
- Type: command registration
- Notes: `rootCmd.AddCommand`


## Java

### Core Java (.java)
- Completed: ✅
- Type: classes, interfaces, enums, methods
- Notes: visibility modifiers

### Spring Boot / Spring MVC
- Completed: ☐
- Type: `@RestController`, `@RequestMapping`
- Notes: `@SpringBootApplication`

### Micronaut / Quarkus
- Completed: ☐
- Type: controller annotations, DI
- Notes: bean lifecycle hooks

### JAX-RS / Jakarta EE
- Completed: ☐
- Type: resource classes
- Notes: `@Path`, `@GET`

### Android
- Completed: ☐
- Type: `Activity`, `Service`, `BroadcastReceiver`
- Notes: manifest-driven entry


## Kotlin

### Core Kotlin (.kt/.kts)
- Completed: ✅
- Type: classes, functions, objects
- Notes: visibility modifiers

### Ktor
- Completed: ☐
- Type: routing blocks
- Notes: `routing {}`

### Spring Boot
- Completed: ☐
- Type: annotations as Java
- Notes: shared Spring patterns

### Android
- Completed: ☐
- Type: activities/fragments
- Notes: Compose entry points

### Jetpack Compose
- Completed: ☐
- Type: `@Composable`
- Notes: `setContent {}`


## C

### Core C (.c/.h)
- Completed: ✅
- Type: functions, structs, enums
- Notes: `extern` detection

### Embedded / RTOS
- Completed: ☐
- Type: ISR handlers, init hooks
- Notes: startup symbols, linker entrypoints


## C++

### Core C++ (.cpp/.cc/.cxx/.hpp/.hxx/.hh)
- Completed: ✅
- Type: classes, templates, RAII patterns
- Notes: constructors/destructors

### Unreal Engine
- Completed: ☐
- Type: gameplay classes, module startup
- Notes: `AActor`, `UGameInstance`, reflection macros

### SDL / GLFW / Qt
- Completed: ☐
- Type: app init and event loop
- Notes: `main` + framework init


## Assembly (Generic)

### Core ASM (.s/.asm/.S)
- Completed: ✅ (structural)
- Type: instruction blocks, labels, directives
- Notes: structural parsing only


## Shell / Bash

### Core Shell (.sh/.bash/.zsh/.fish)
- Completed: ✅ (structural)
- Type: functions, variable assignments
- Notes: entry is script invocation


## Gradle (Groovy)

### Core Gradle (.gradle)
- Completed: ✅ (structural)
- Type: task definitions, plugins
- Notes: build graph entry points


## C# / .NET

### Core C#
- Completed: ✅
- Type: classes, methods, attributes
- Notes: visibility modifiers

### ASP.NET Core MVC
- Completed: ☐ (boilerplate detector exists, not entry-point semantics)
- Type: controllers/actions, attribute routing
- Notes: `ControllerBase`, `HttpGet/HttpPost` attributes

### ASP.NET Minimal APIs
- Completed: ☐ (boilerplate detector exists, not entry-point semantics)
- Type: `MapGet/MapPost/MapGroup` handlers
- Notes: `Program.cs` is implicit entry

### Razor Pages
- Completed: ☐
- Type: `PageModel` handlers
- Notes: `OnGet/OnPost` methods

### Blazor (Server/WASM)
- Completed: ☐
- Type: routed components, `App.razor`
- Notes: `@page` directives

### gRPC
- Completed: ☐
- Type: service implementations, method handlers
- Notes: `GrpcService` base classes

### Azure Functions
- Completed: ☐
- Type: `[FunctionName]` methods
- Notes: trigger attributes

### Unity
- Completed: ☐ (boilerplate detector exists, not entry-point semantics)
- Type: MonoBehaviour lifecycle, ScriptableObject creation
- Notes: `Start`, `Update`, `Awake`, scene entry

### Godot (C#)
- Completed: ☐
- Type: `_Ready`, `_Process`, `_PhysicsProcess`
- Notes: node lifecycle

### Xamarin / MAUI
- Completed: ☐
- Type: app entry, page routes
- Notes: platform lifecycle methods

### Entity Framework
- Completed: ☐
- Type: `DbContext` configuration
- Notes: model builder patterns


## Swift

### Core Swift
- Completed: ☐ (language not in README yet)
- Type: functions, types
- Notes: pending parser

### SwiftUI
- Completed: ☐
- Type: `@main` App struct, `Scene`
- Notes: declarative entry

### Vapor
- Completed: ☐
- Type: route registration
- Notes: `app.get/post`


## PHP

### Core PHP
- Completed: ☐ (language not in README yet)
- Type: functions, classes
- Notes: pending parser

### Laravel / Symfony
- Completed: ☐
- Type: controllers, routes, console commands
- Notes: framework bootstraps

### WordPress
- Completed: ☐
- Type: hooks, actions, filters
- Notes: plugin entry points


## Vue SFC

### Vue Single-File Components (.vue)
- Completed: ✅
- Type: SFC script extraction
- Notes: language-aware parsing


## Odin (not yet supported)

### Odin core
- Completed: ☐
- Type: `package main`, `proc main()`
- Notes: packages are directories with a single package name; entry point is `proc main()` in `package main`

### Odin game libs
- Completed: ☐
- Type: entry + game loop registration
- Notes: raylib bindings, ECS frameworks


## Dreamcast (C / C++ + KallistiOS)

### KallistiOS (KOS)
- Completed: ☐
- Type: `main`, hardware init, `arch_init`, `fs_init`
- Notes: KOS startup sequence; drivers and subsystem init are implicit entry points

### KallistiOS Addons / Libraries
- Completed: ☐
- Type: audio/video/input init routines
- Notes: `snd_stream_init`, `pvr_init`, `maple_init`

### Homebrew toolchains
- Completed: ☐
- Type: linker entry (`_start`), crt0
- Notes: `libdream` / `newlib` startup


## Build Systems & Tooling Languages (README)

### Makefile
- Completed: ✅ (structural parsing)
- Type: targets and dependencies
- Notes: build graph entry points

### CMake
- Completed: ✅ (structural parsing)
- Type: target definitions
- Notes: build graph entry points

### GNU Linker Scripts
- Completed: ◻
- Type: linker entry + sections
- Notes: structural parsing only

### GCC Attributes & Pragmas
- Completed: ✅ (as part of C/C++ AST)
- Type: annotations
- Notes: compiler control


## Markup & Styling (README)

### HTML
- Completed: ◻
- Type: document structure
- Notes: structural parsing only

### CSS
- Completed: ◻
- Type: stylesheet rules
- Notes: structural parsing only

### SCSS / SASS
- Completed: ◻
- Type: nested stylesheet rules
- Notes: structural parsing only

### Markdown
- Completed: ◻
- Type: section/block structure
- Notes: structural parsing only


## Configuration & Data (README)

### JSON
- Completed: ◻
- Type: structural parsing
- Notes: config/data only

### YAML
- Completed: ◻
- Type: structural parsing
- Notes: config/data only

### TOML
- Completed: ◻
- Type: structural parsing
- Notes: config/data only

### XML
- Completed: ◻
- Type: structural parsing
- Notes: config/data only

### HCL / Terraform
- Completed: ✅ (structural parsing)
- Type: IaC parsing
- Notes: no framework semantics yet
