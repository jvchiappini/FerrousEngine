//! Sistema de animaciones para widgets de Ferrous UI.
//!
//! Proporciona herramientas para crear animaciones suaves sin código manual de lerping.
//! Uso típico dentro de `Widget::update`:
//!
//! ```rust,ignore
//! struct MyWidget {
//!     opacity: Animated<f32>,
//!     pos: Spring<[f32; 2]>,
//!     slide: Tween<f32>,
//! }
//!
//! impl<App> Widget<App> for MyWidget {
//!     fn update(&mut self, ctx: &mut UpdateContext) {
//!         if self.opacity.tick(ctx.delta_time) { ctx.needs_redraw = true; }
//!         if self.pos.tick(ctx.delta_time)     { ctx.needs_redraw = true; }
//!         if self.slide.tick(ctx.delta_time)   { ctx.needs_redraw = true; }
//!     }
//! }
//! ```

// ─── Trait Lerp ──────────────────────────────────────────────────────────────

/// Interpolación lineal entre dos valores.
/// Implementado para los tipos más comunes de la UI.
pub trait Lerp: Copy + PartialEq {
    fn lerp(a: Self, b: Self, t: f32) -> Self;
    fn is_close(a: Self, b: Self, epsilon: f32) -> bool;
}

impl Lerp for f32 {
    #[inline] fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
    #[inline] fn is_close(a: f32, b: f32, eps: f32) -> bool { (b - a).abs() < eps }
}

impl Lerp for [f32; 2] {
    fn lerp(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
        [f32::lerp(a[0], b[0], t), f32::lerp(a[1], b[1], t)]
    }
    fn is_close(a: [f32; 2], b: [f32; 2], eps: f32) -> bool {
        f32::is_close(a[0], b[0], eps) && f32::is_close(a[1], b[1], eps)
    }
}

impl Lerp for [f32; 4] {
    fn lerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
        [
            f32::lerp(a[0], b[0], t),
            f32::lerp(a[1], b[1], t),
            f32::lerp(a[2], b[2], t),
            f32::lerp(a[3], b[3], t),
        ]
    }
    fn is_close(a: [f32; 4], b: [f32; 4], eps: f32) -> bool {
        (0..4).all(|i| f32::is_close(a[i], b[i], eps))
    }
}

impl Lerp for glam::Vec2 {
    fn lerp(a: glam::Vec2, b: glam::Vec2, t: f32) -> glam::Vec2 {
        a + (b - a) * t
    }
    fn is_close(a: glam::Vec2, b: glam::Vec2, eps: f32) -> bool {
        (b - a).length() < eps
    }
}

// ─── Animated<T> ─────────────────────────────────────────────────────────────

/// Valor que se aproxima exponencialmente a su objetivo en cada frame.
///
/// Ideal para transiciones suaves de hover, color, opacidad, etc.
/// La velocidad de convergencia es `speed` (unidades/segundo equivalentes).
///
/// ```rust,ignore
/// let mut opacity = Animated::new(0.0, 8.0); // 8 = velocidad de transición
/// opacity.set_target(1.0);
/// // En update:
/// if opacity.tick(delta_time) { needs_redraw = true; }
/// let current_opacity = opacity.value();
/// ```
#[derive(Clone, Debug)]
pub struct Animated<T: Lerp> {
    current: T,
    target: T,
    /// Velocidad de convergencia. Mayor = más rápido. 8.0 es fluido, 20.0 es casi inmediato.
    speed: f32,
    /// Tolerancia para considerar que la animación terminó.
    epsilon: f32,
    /// `true` si el valor aún está moviéndose hacia el target.
    is_animating: bool,
}

impl<T: Lerp> Animated<T> {
    pub fn new(initial: T, speed: f32) -> Self {
        Self {
            current: initial,
            target: initial,
            speed,
            epsilon: 0.001,
            is_animating: false,
        }
    }

    pub fn with_epsilon(mut self, eps: f32) -> Self {
        self.epsilon = eps;
        self
    }

    /// Establece el valor objetivo. La animación comienza inmediatamente.
    pub fn set_target(&mut self, target: T) {
        if target != self.target {
            self.target = target;
            self.is_animating = true;
        }
    }

    /// Salta directamente al valor sin animación.
    pub fn set_immediate(&mut self, value: T) {
        self.current = value;
        self.target = value;
        self.is_animating = false;
    }

    /// Avanza la animación. Retorna `true` si el valor cambió (necesita redibujado).
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.is_animating {
            return false;
        }
        let t = 1.0 - (-self.speed * dt).exp(); // exponential decay
        let new = T::lerp(self.current, self.target, t.clamp(0.0, 1.0));
        self.current = new;
        if T::is_close(self.current, self.target, self.epsilon) {
            self.current = self.target;
            self.is_animating = false;
        }
        true
    }

    /// Valor actual interpolado.
    #[inline] pub fn value(&self) -> T { self.current }

    /// Valor objetivo.
    #[inline] pub fn target(&self) -> T { self.target }

    /// `true` si la animación está en curso.
    #[inline] pub fn is_animating(&self) -> bool { self.is_animating }
}

// ─── Spring<T> ───────────────────────────────────────────────────────────────

/// Animación basada en simulación de resorte físico.
///
/// Produce animaciones con overshoot natural — ideal para menús emergentes,
/// tooltips, drag & drop. La respuesta es independiente del framerate.
///
/// ```rust,ignore
/// let mut scale = Spring::<f32>::new(1.0, 300.0, 20.0); // stiffness=300, damping=20
/// scale.set_target(1.1); // El valor "rebota" ligeramente antes de asentarse
/// ```
#[derive(Clone, Debug)]
pub struct Spring<T: Lerp + Default> {
    current: T,
    target: T,
    velocity: f32,
    stiffness: f32,
    damping: f32,
    epsilon: f32,
}

impl Spring<f32> {
    pub fn new(initial: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            current: initial,
            target: initial,
            velocity: 0.0,
            stiffness,
            damping,
            epsilon: 0.001,
        }
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    pub fn set_immediate(&mut self, value: f32) {
        self.current = value;
        self.target = value;
        self.velocity = 0.0;
    }

    /// Avanza la simulación. Retorna `true` si el valor cambió.
    pub fn tick(&mut self, dt: f32) -> bool {
        let force = -self.stiffness * (self.current - self.target);
        let damping_force = -self.damping * self.velocity;
        let acceleration = force + damping_force;
        self.velocity += acceleration * dt;
        self.current += self.velocity * dt;

        let settled = (self.current - self.target).abs() < self.epsilon
            && self.velocity.abs() < self.epsilon;
        if settled {
            self.current = self.target;
            self.velocity = 0.0;
            return false; // ya no necesita actualizarse
        }
        true
    }

    #[inline] pub fn value(&self) -> f32 { self.current }
    #[inline] pub fn target(&self) -> f32 { self.target }
    #[inline] pub fn velocity(&self) -> f32 { self.velocity }
}

// ─── Easing ───────────────────────────────────────────────────────────────────

/// Funciones de easing para `Tween`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Rebote: el valor va ligeramente más allá del target y regresa.
    EaseOutBack,
    /// Elástico: múltiples oscilaciones antes de asentarse.
    EaseOutElastic,
}

impl Easing {
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear      => t,
            Self::EaseIn      => t * t,
            Self::EaseOut     => 1.0 - (1.0 - t).powi(2),
            Self::EaseInOut   => {
                if t < 0.5 { 2.0 * t * t }
                else { 1.0 - (-2.0 * t + 2.0).powi(2) * 0.5 }
            }
            Self::EaseOutBack => {
                let c1 = 1.70158_f32;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
            Self::EaseOutElastic => {
                if t == 0.0 { return 0.0; }
                if t == 1.0 { return 1.0; }
                let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
            }
        }
    }
}

// ─── Tween<T> ────────────────────────────────────────────────────────────────

/// Animación keyframed: de `from` a `to` en `duration` segundos con una curva de easing.
///
/// ```rust,ignore
/// let mut tween = Tween::new(0.0, 1.0, 0.3, Easing::EaseOutBack);
/// // En update:
/// if tween.tick(delta_time) { needs_redraw = true; }
/// let value = tween.value();
/// ```
#[derive(Clone, Debug)]
pub struct Tween<T: Lerp> {
    from: T,
    to: T,
    duration: f32,
    elapsed: f32,
    easing: Easing,
    done: bool,
    /// Si `true`, la animación se repite cíclicamente.
    pub looping: bool,
}

impl<T: Lerp> Tween<T> {
    pub fn new(from: T, to: T, duration: f32, easing: Easing) -> Self {
        Self {
            from,
            to,
            duration: duration.max(0.001),
            elapsed: 0.0,
            easing,
            done: false,
            looping: false,
        }
    }

    /// Reinicia la animación desde el principio.
    pub fn restart(&mut self) {
        self.elapsed = 0.0;
        self.done = false;
    }

    /// Invierte la animación (intercambia `from` y `to` y reinicia).
    pub fn reverse(&mut self) {
        std::mem::swap(&mut self.from, &mut self.to);
        self.elapsed = 0.0;
        self.done = false;
    }

    /// Avanza la animación. Retorna `true` si no ha terminado.
    pub fn tick(&mut self, dt: f32) -> bool {
        if self.done { return false; }
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            if self.looping {
                self.elapsed -= self.duration;
            } else {
                self.elapsed = self.duration;
                self.done = true;
            }
        }
        true
    }

    /// Valor interpolado actual.
    pub fn value(&self) -> T {
        let t_raw = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let t = self.easing.apply(t_raw);
        T::lerp(self.from, self.to, t)
    }

    #[inline] pub fn is_done(&self) -> bool { self.done }
    #[inline] pub fn progress(&self) -> f32 { (self.elapsed / self.duration).clamp(0.0, 1.0) }
}
