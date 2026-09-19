# quill component demo

Shows the recommended, reusable component API:

```rust
let mut ui = Ui::new();

let panel = ui.add(ui.root(), Panel::new());
let vbox = ui.add(panel.id(), VBox::new());
ui.add(vbox.id(), Label::new("Hello"));

let clicks = Rc::new(Cell::new(0));
let counter = clicks.clone();
let button = ui.add(
    vbox.id(),
    Button::new("Click me").on_click(move || counter.set(counter.get() + 1)),
);
let status = ui.add(vbox.id(), Label::new("Status: Clicked 0 times"));

// each frame:
ui.set_text(status.id(), format!("Status: Clicked {} times", clicks.get()));
ui.layout(viewport);
```

## Build & run

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128

./build.sh
python3 -m http.server 8080 --directory .
# open http://localhost:8080/
```

## What it shows

- creating components (`Panel`, `VBox`, `Label`, `Button`),
- composing them into a tree,
- layout via anchors/offsets and a `VBox`,
- event handling with `Button::on_click`,
- state change (`Rc<Cell<u32>>`) reflected back into the UI,
- attaching the app to Canvas/WASM via `draw_wasm::start`.

Open with `?selftest=1` to dispatch real pointer events at the button headlessly.
