# 30 Frontend Policy

This policy applies to frontend code in this repository.

It is optimized for AI agents working on a Tauri + React + TypeScript frontend for `cryptovol`.

This policy is subordinate to:

* `00-engineering-policy.md`
* `10-rust-project-structure-policy.md`
* `20-rust-code-policy.md`

When policies conflict, the more specific policy wins for its scope.

## 1. Scope

This policy applies to:

```text
apps/cryptovol-gui/src/
```

and any future frontend-only TypeScript/React code.

This policy does not apply to:

```text
crates/
apps/cryptovol-gui/src-tauri/
```

Rust/Tauri backend code is governed by Rust and Tauri-specific policies.

## 2. Frontend Goals

The frontend MUST be a thin UI layer over `cryptovol-app`.

The frontend MUST NOT reimplement:

* TC/VC container logic
* crypto/KDF/PIM logic
* filesystem parsing
* path resolution semantics
* extraction logic
* progress/cancellation backend logic
* overwrite/symlink/tempfile safety logic

The frontend MAY manage:

* form state
* selected container path
* selected file/directory
* current directory path
* loading/error state
* progress display state
* simple UI-only validation

## 3. Technology Baseline

Frontend code MUST use:

* TypeScript
* React function components
* Vite
* strict TypeScript settings

Frontend code SHOULD use:

* Material UI for basic UI components
* MUI icons when icons are useful

Frontend code MUST NOT use:

* JavaScript files for app logic
* React class components
* Redux
* Zustand
* React Router
* MUI X Pro/Premium
* Tailwind
* shadcn/ui
* a second UI component framework

Exceptions require explicit human approval.

## 4. TypeScript Rules

TypeScript MUST be strict.

Code MUST NOT use:

```ts
any
```

except in narrowly isolated interop code with a comment explaining why.

Prefer:

```ts
unknown
```

over:

```ts
any
```

Types MUST be explicit at frontend/backend boundaries.

Tauri command inputs and outputs MUST have named DTO types.

Do not infer command payload shapes inline inside React components.

Use discriminated unions for state machines and async states.

Example:

```ts
type OpenVolumeState =
  | { status: "idle" }
  | { status: "opening" }
  | { status: "opened"; session: VolumeSessionDto }
  | { status: "failed"; error: GuiErrorDto };
```

## 5. Security Rules

The frontend MUST NOT log secrets.

The frontend MUST NOT store secrets in:

* `localStorage`
* `sessionStorage`
* IndexedDB
* URL parameters
* window title
* console logs
* persistent app settings

Passwords MUST only live in short-lived component state.

Password state SHOULD be cleared after successful container open.

Password inputs MUST use:

```tsx
type="password"
```

Progress events MUST NOT include:

* passwords
* derived keys
* decrypted file contents
* decrypted header bytes
* raw binary dumps

Error views MUST display sanitized user-facing errors only.

Do not render raw backend debug output directly.

## 6. Frontend Architecture

Use a pragmatic Feature-Sliced-style structure.

The frontend source tree MUST use this top-level structure:

```text
src/
  app/
  pages/
  widgets/
  features/
  entities/
  shared/
```

Do not add new top-level folders without explicit approval.

### 6.1 Layer Responsibilities

#### `app/`

`app/` contains application bootstrap code.

Allowed contents:

```text
app/
  App.tsx
  main.tsx
  providers/
  theme/
  styles/
```

`app/` MAY import from all other layers.

`app/` MUST NOT contain business logic.

#### `pages/`

`pages/` contains full-screen page compositions.

For the GUI MVP, likely pages are:

```text
pages/
  open-volume/
  volume-browser/
```

Pages MAY compose widgets, features, entities, and shared code.

Pages MUST NOT call Tauri commands directly.

Pages MUST NOT implement extraction or filesystem logic.

#### `widgets/`

`widgets/` contains large self-contained UI sections.

Examples:

```text
widgets/
  volume-info-panel/
  directory-browser/
  extraction-panel/
  app-shell/
```

Widgets MAY compose features, entities, and shared code.

Widgets MUST NOT call Tauri commands directly unless the command is purely UI-adjacent and approved.

Prefer calling feature-level APIs/actions.

#### `features/`

`features/` contains user actions that provide product value.

Examples:

```text
features/
  open-container/
  close-session/
  browse-directory/
  extract-file/
  cancel-extraction/
```

Features MAY call the command client.

Features MAY own UI state related to a user action.

Features MUST NOT contain broad app layout code.

Features MUST NOT import from other feature slices.

#### `entities/`

`entities/` contains frontend representations of domain concepts.

Examples:

```text
entities/
  volume/
  file-entry/
  extraction-job/
```

Entities MAY contain:

```text
model/
ui/
lib/
```

Entities MUST NOT call Tauri commands.

Entities MUST NOT import from features, widgets, or pages.

#### `shared/`

`shared/` contains reusable code that is not domain-specific.

Required shared structure:

```text
shared/
  api/
  config/
  lib/
  ui/
  types/
```

`shared/api/` contains the Tauri command client and DTO definitions.

`shared/ui/` contains generic reusable UI components.

`shared/lib/` contains small pure helpers.

`shared/config/` contains constants and configuration.

`shared/types/` contains generic types not tied to one entity.

`shared/` MUST NOT import from any other frontend layer.

## 7. Import Direction

Imports MUST follow this direction:

```text
app
  -> pages
    -> widgets
      -> features
        -> entities
          -> shared
```

A layer MAY import from layers below it.

A layer MUST NOT import from layers above it.

Slices on the same layer MUST NOT import from each other.

Forbidden examples:

```ts
// forbidden: entity importing feature
import { extractFile } from "@/features/extract-file";

// forbidden: feature importing widget
import { DirectoryBrowser } from "@/widgets/directory-browser";

// forbidden: shared importing entity
import { FileEntry } from "@/entities/file-entry";
```

Allowed examples:

```ts
// allowed: feature importing shared API client
import { openContainerCommand } from "@/shared/api/commands";

// allowed: widget importing feature UI/action
import { ExtractFileButton } from "@/features/extract-file";

// allowed: page importing widget
import { DirectoryBrowser } from "@/widgets/directory-browser";
```

Use path aliases.

Required alias:

```ts
@/* -> src/*
```

Avoid deep relative imports across slices.

Prefer:

```ts
import { DirectoryBrowser } from "@/widgets/directory-browser";
```

over:

```ts
import { DirectoryBrowser } from "../../widgets/directory-browser/ui/DirectoryBrowser";
```

## 8. Slice Structure

Each slice SHOULD use this structure when needed:

```text
slice-name/
  index.ts
  ui/
  model/
  api/
  lib/
```

Use only the segments needed.

Do not create empty folders.

### `index.ts`

Each slice SHOULD expose a small public API through `index.ts`.

Other slices SHOULD import from the slice root.

Good:

```ts
import { ExtractFileButton } from "@/features/extract-file";
```

Avoid:

```ts
import { ExtractFileButton } from "@/features/extract-file/ui/ExtractFileButton";
```

### `ui/`

Contains React components.

Components MUST be small and focused.

A component SHOULD stay below 200 lines.

A component MUST NOT exceed 300 lines without explicit justification.

### `model/`

Contains local state, hooks, reducers, view models, and action orchestration.

Complex state MUST be moved out of UI components into `model/`.

### `api/`

Contains slice-specific API adapters.

Most Tauri command calls SHOULD remain centralized in:

```text
shared/api/
```

Use slice `api/` only for thin feature-specific wrappers.

### `lib/`

Contains pure helper functions for that slice.

Helpers in `lib/` MUST NOT depend on React unless clearly UI-specific.

## 9. Tauri Command Client

All frontend calls to Tauri commands MUST go through:

```text
shared/api/
```

Required file pattern:

```text
shared/api/
  commands.ts
  dto.ts
  errors.ts
```

React components MUST NOT call `invoke` directly.

Bad:

```tsx
const result = await invoke("open_container", payload);
```

Good:

```tsx
const result = await openContainer(payload);
```

Command DTOs MUST be defined in `shared/api/dto.ts` or a clearly equivalent file.

Command errors MUST be normalized into a typed frontend error model.

## 10. React Rules

Use function components only.

Use hooks only at the top level.

Do not create hooks that hide backend side effects unless their name clearly communicates it.

Hooks that call backend commands SHOULD live in feature `model/` folders.

Components MUST NOT perform complex async orchestration inline.

Bad:

```tsx
function OpenButton() {
  async function onClick() {
    // 80 lines of command calls, error mapping, state transitions
  }
}
```

Good:

```tsx
function OpenButton() {
  const { open, state } = useOpenContainer();
}
```

## 11. State Management

For the GUI MVP, use React built-in state:

* `useState`
* `useReducer`
* `useMemo`
* `useCallback`
* `useEffect`

Do not add global state libraries without explicit approval.

Allowed app state:

```text
current session
current path
directory entries
selected entry
extraction job state
current error
loading state
```

State containing passwords MUST remain local and short-lived.

State machines SHOULD use discriminated unions.

## 12. Material UI Rules

Use Material UI for standard controls.

Allowed MUI components include:

* `Button`
* `TextField`
* `Select`
* `MenuItem`
* `Dialog`
* `Table`
* `LinearProgress`
* `Alert`
* `Card`
* `Toolbar`
* `Typography`
* `Box`
* `Stack`

Do not add MUI X Pro/Premium.

Do not add a second design system.

Do not build custom controls when a standard MUI component is sufficient.

Keep styling simple.

Prefer MUI `sx` for small local styling.

Extract repeated styling into small components only when duplication appears.

Do not create a large custom theme in the MVP.

## 13. Error Handling

Frontend errors MUST use a typed model.

Example:

```ts
export type GuiError = {
  code: string;
  message: string;
};
```

UI MUST show friendly error messages.

UI MUST NOT display raw stack traces to users.

Developer-only details MAY be logged only if they contain no secrets.

Never log passwords or decrypted data.

## 14. Progress and Cancellation

Progress state SHOULD be represented explicitly.

Example:

```ts
type ExtractionState =
  | { status: "idle" }
  | { status: "running"; jobId: string; bytesWritten: number; totalBytes?: number }
  | { status: "finished"; bytesWritten: number }
  | { status: "cancelled" }
  | { status: "failed"; error: GuiError };
```

Cancellation MUST call the backend cancellation command.

Do not fake cancellation only in the UI.

Progress events MUST be unsubscribed/cleaned up when components unmount.

## 15. File and Path Handling

Frontend path handling MUST remain minimal.

The frontend MAY store and display paths.

The frontend MUST NOT normalize encrypted-volume paths.

The frontend MUST NOT rewrite Unicode paths.

The frontend MUST preserve paths received from backend entries.

Host filesystem destination paths SHOULD come from file dialogs or explicit user input.

Do not invent platform-specific path parsing in React unless necessary.

## 16. Testing

Frontend code SHOULD have lightweight tests for pure logic.

Test:

* DTO mapping
* error mapping
* state reducers
* progress state transitions
* validation helpers

Do not add heavy UI test infrastructure in the first GUI MVP unless already present.

At minimum, the frontend MUST pass:

```bash
npm run build
```

and, if configured:

```bash
npm run typecheck
npm test
```

## 17. Logging

Do not use `console.log` in committed frontend code except temporary debug statements removed before final report.

Allowed:

```ts
console.error(sanitizedError);
```

only if it cannot contain secrets.

Prefer visible UI errors over console output.

## 18. Accessibility

Basic accessibility is required.

Controls MUST have visible labels.

Icon-only buttons MUST have accessible labels.

Password fields MUST have labels.

Progress indicators SHOULD have accessible text when practical.

Do not use color alone to communicate errors or success.

## 19. Performance

Do not render huge directory tables inefficiently on purpose.

For the MVP, a normal MUI table is acceptable.

Do not add virtualization unless directories with thousands of entries become a proven problem.

Avoid expensive recomputation in render loops.

Use memoization only when it improves clarity or solves an observed issue.

## 20. Documentation

When adding frontend structure, update or create documentation that explains:

* frontend stack
* folder structure
* import direction
* Tauri command client
* where UI state lives
* how to run frontend checks

## 21. Final Report Requirements

When a task changes frontend code, the final report MUST include:

* frontend files changed
* new slices/layers added
* whether import direction was preserved
* whether secrets are handled safely
* checks run:

    * `npm run build`
    * `npm run typecheck`, if available
    * `npm test`, if available
* any intentionally deferred frontend work