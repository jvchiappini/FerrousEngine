# ferrous_ui_macros

`ferrous_ui_macros` provee las macros de procedimiento esenciales para simplificar la construcción de interfaces en FerrousEngine. Su objetivo principal es ofrecer una Developer Experience (DX) superior mediante sintaxis declarativa.

---

## Macros Incluidas

### `ui!` (Macro de Construcción Declarativa)

La macro `ui!` permite describir la jerarquía de la interfaz de forma estructural y anidada, eliminando el código imperativo repetitivo.

#### Sintaxis Básica

```rust
let root_id = ui! {
    Panel(Color::RED) {
        Label("Título")
        Button("Click")
    }
};
```

#### Cómo funciona

La macro traduce el árbol declarativo en llamadas directas a `ctx.add_child`. Por ejemplo, el código anterior se expande aproximadamente a:

```rust
{
    let __id = ctx.add_child(Box::new(Panel::new(Color::RED)));
    {
        let mut ctx = BuildContext { ... }; // Contexto transitorio para el subárbol
        ctx.add_child(Box::new(Label::new("Título")));
        ctx.add_child(Box::new(Button::new("Click")));
    }
    __id
}
```

---

## Roadmap del Crate (Fase 5.1 & 7.1)

- [ ] **Soporte de Propiedades con Nombre:** Permitir `Panel(color: #1A1A2E, radius: 8)`.
- [ ] **Bindings Directos:** Soportar `Slider(bind: volume)`.
- [ ] **`#[derive(FerrousWidget)]`:** Macro para habilitar la inspección en tiempo real y serialización para el Ferrous Builder.
- [ ] **Fragmentos:** Soportar condicionales y bucles dentro de la macro `ui!`.

---

## Mejores Prácticas

- Usa `ui!` dentro de la implementación de `Widget::build` para definir los hijos de un componente complejo.
- Dado que es una `proc_macro`, los errores de sintaxis se capturan en tiempo de compilación.
