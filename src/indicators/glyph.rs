use mecha_wayland::prelude::*;

pub const GLYPH_DOT_SIZE: f32 = 5.0;
pub const GLYPH_DOT_RADIUS: f32 = 2.5;
pub const GLYPH_DOT_GAP: f32 = 2.0;
pub const GLYPH_COLS: usize = 4;
pub const GLYPH_ROWS: usize = 2;
pub const GLYPH_TOTAL_DOTS: usize = GLYPH_COLS * GLYPH_ROWS;

/// An 8-circle indicator representing a fixed 4x2 dot grid.
///
/// Array layout:
/// - Row 0 (top):    indices [0, 1, 2, 3] (left to right)
/// - Row 1 (bottom): indices [4, 5, 6, 7] (left to right)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphState(pub [bool; 8]);

impl Default for GlyphState {
    fn default() -> Self {
        Self::DEMO
    }
}

impl GlyphState {
    pub const EMPTY: Self = Self([false; 8]);
    pub const ALL: Self = Self([true; 8]);

    /// Specification demo pattern:
    /// - Row 0: [●, ●, ●, ○]
    /// - Row 1: [●, ○, ●, ●]
    pub const DEMO: Self = Self([true, true, true, false, true, false, true, true]);

    #[inline]
    pub const fn new(dots: [bool; 8]) -> Self {
        Self(dots)
    }

    #[inline]
    pub const fn dots(&self) -> [bool; 8] {
        self.0
    }

    #[inline]
    pub const fn is_filled(&self, index: usize) -> bool {
        if index < 8 { self.0[index] } else { false }
    }

    #[inline]
    pub const fn is_filled_2d(&self, row: usize, col: usize) -> bool {
        if row < 2 && col < 4 {
            self.0[row * 4 + col]
        } else {
            false
        }
    }

    #[inline]
    pub fn set_dot(&mut self, index: usize, filled: bool) {
        if index < 8 {
            self.0[index] = filled;
        }
    }

    #[inline]
    pub fn set_dot_2d(&mut self, row: usize, col: usize, filled: bool) {
        if row < 2 && col < 4 {
            self.0[row * 4 + col] = filled;
        }
    }
}

impl From<[bool; 8]> for GlyphState {
    #[inline]
    fn from(dots: [bool; 8]) -> Self {
        Self(dots)
    }
}

impl From<GlyphState> for [bool; 8] {
    #[inline]
    fn from(state: GlyphState) -> Self {
        state.0
    }
}

impl From<[[bool; 4]; 2]> for GlyphState {
    #[inline]
    fn from(grid: [[bool; 4]; 2]) -> Self {
        Self([
            grid[0][0], grid[0][1], grid[0][2], grid[0][3], grid[1][0], grid[1][1], grid[1][2],
            grid[1][3],
        ])
    }
}

impl From<GlyphState> for [[bool; 4]; 2] {
    #[inline]
    fn from(state: GlyphState) -> Self {
        [
            [state.0[0], state.0[1], state.0[2], state.0[3]],
            [state.0[4], state.0[5], state.0[6], state.0[7]],
        ]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GlyphIndicator {
    pub state: GlyphState,
    pub dots: [Handle<Div>; 8],
}

pub fn glyph_indicator() -> GlyphIndicatorBuilder {
    GlyphIndicatorBuilder::default()
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GlyphIndicatorBuilder {
    pub state: GlyphState,
}

impl GlyphIndicatorBuilder {
    pub fn state(mut self, state: impl Into<GlyphState>) -> Self {
        self.state = state.into();
        self
    }
}

impl Build for GlyphIndicatorBuilder {
    type Widget = GlyphIndicator;
}

impl Widget for GlyphIndicator {
    type Builder = GlyphIndicatorBuilder;

    fn build(b: GlyphIndicatorBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default()
            .column()
            .center()
            .gap(px(GLYPH_DOT_GAP));

        let primary = s.color(ColorRole::Primary);
        let secondary = s.color(ColorRole::SecondaryFixedDim);

        let row_style = LayoutStyle::default().row().gap(px(GLYPH_DOT_GAP));
        let r0 = s.spawn(me, div().style(row_style.clone()));
        let r1 = s.spawn(me, div().style(row_style));

        let dot_style = LayoutStyle::default().size(px(GLYPH_DOT_SIZE), px(GLYPH_DOT_SIZE));
        let dots = std::array::from_fn(|i| {
            let parent = if i < 4 { r0 } else { r1 };
            let color = if b.state.0[i] { primary } else { secondary };
            s.spawn(
                parent,
                div()
                    .style(dot_style.clone())
                    .background(color)
                    .radius(GLYPH_DOT_RADIUS),
            )
        });

        s.on_theme(me, move |ctx| {
            let primary = ctx.color(ColorRole::Primary);
            let secondary = ctx.color(ColorRole::SecondaryFixedDim);
            let state = ctx.me().state;
            for (i, &dot) in dots.iter().enumerate() {
                let color = if state.0[i] { primary } else { secondary };
                ctx.at(dot).unwrap().set_background(color);
            }
        });

        GlyphIndicator {
            state: b.state,
            dots,
        }
    }
}

impl GlyphIndicator {
    #[inline]
    pub fn state(&self) -> GlyphState {
        self.state
    }
}

pub trait GlyphIndicatorContext {
    fn state(&mut self) -> GlyphState;
    fn set_state(&mut self, state: impl Into<GlyphState>);
}

impl GlyphIndicatorContext for Context<'_, GlyphIndicator> {
    #[inline]
    fn state(&mut self) -> GlyphState {
        self.me().state
    }

    fn set_state(&mut self, state: impl Into<GlyphState>) {
        let state = state.into();
        if self.me().state == state {
            return;
        }
        self.me().state = state;

        let primary = self.color(ColorRole::Primary);
        let secondary = self.color(ColorRole::SecondaryFixedDim);
        let dots = self.me().dots;

        for (i, &dot) in dots.iter().enumerate() {
            let color = if state.0[i] { primary } else { secondary };
            self.at(dot).unwrap().set_background(color);
        }
    }
}
