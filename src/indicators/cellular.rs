use mecha_wayland::prelude::*;

use crate::indicators::{indicator_layout, load_svg_icon};

mod svg {
    pub const FULL: &[u8] = include_bytes!("../../assets/cellular/cell-signal-full.svg");
    pub const HIGH: &[u8] = include_bytes!("../../assets/cellular/cell-signal-high.svg");
    pub const MEDIUM: &[u8] = include_bytes!("../../assets/cellular/cell-signal-medium.svg");
    pub const LOW: &[u8] = include_bytes!("../../assets/cellular/cell-signal-low.svg");
    pub const NONE: &[u8] = include_bytes!("../../assets/cellular/cell-signal-none.svg");
    pub const SLASH: &[u8] = include_bytes!("../../assets/cellular/cell-signal-slash.svg");
    pub const X: &[u8] = include_bytes!("../../assets/cellular/cell-signal-x.svg");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellularState {
    #[default]
    Full,
    High,
    Medium,
    Low,
    None,
    Disabled,
    Disconnected,
}

impl CellularState {
    #[inline]
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Disabled | Self::None)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CellularIcons {
    pub full: SpriteId,
    pub high: SpriteId,
    pub medium: SpriteId,
    pub low: SpriteId,
    pub none: SpriteId,
    pub slash: SpriteId,
    pub x: SpriteId,
}

impl CellularIcons {
    pub fn load(atlas: &mut Atlas, master: u32) -> Result<Self, Error> {
        Ok(Self {
            full: load_svg_icon(atlas, svg::FULL, master)?,
            high: load_svg_icon(atlas, svg::HIGH, master)?,
            medium: load_svg_icon(atlas, svg::MEDIUM, master)?,
            low: load_svg_icon(atlas, svg::LOW, master)?,
            none: load_svg_icon(atlas, svg::NONE, master)?,
            slash: load_svg_icon(atlas, svg::SLASH, master)?,
            x: load_svg_icon(atlas, svg::X, master)?,
        })
    }

    pub fn sprite(&self, state: CellularState) -> SpriteId {
        match state {
            CellularState::Full => self.full,
            CellularState::High => self.high,
            CellularState::Medium => self.medium,
            CellularState::Low => self.low,
            CellularState::None => self.none,
            CellularState::Disabled => self.slash,
            CellularState::Disconnected => self.x,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CellularIndicator {
    pub icons: CellularIcons,
    pub state: CellularState,
    pub icon_node: Handle<Icon>,
}

pub fn cellular_indicator(icons: CellularIcons) -> CellularIndicatorBuilder {
    CellularIndicatorBuilder {
        icons,
        state: CellularState::default(),
    }
}

pub struct CellularIndicatorBuilder {
    pub icons: CellularIcons,
    pub state: CellularState,
}

impl CellularIndicatorBuilder {
    pub fn state(mut self, state: CellularState) -> Self {
        self.state = state;
        self
    }
}

impl Build for CellularIndicatorBuilder {
    type Widget = CellularIndicator;
}

impl Widget for CellularIndicator {
    type Builder = CellularIndicatorBuilder;

    fn build(b: CellularIndicatorBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = indicator_layout(b.state.is_visible());

        let sprite = b.icons.sprite(b.state);
        let color = s.color(ColorRole::OnSurface);
        let icon_node = s.spawn(me, icon(sprite).color(color));

        s.on_theme(me, move |ctx| {
            let color = ctx.color(ColorRole::OnSurface);
            ctx.at(icon_node).unwrap().set_color(color);
        });

        CellularIndicator {
            icons: b.icons,
            state: b.state,
            icon_node,
        }
    }
}

impl CellularIndicator {
    #[inline]
    pub fn state(&self) -> CellularState {
        self.state
    }
}

pub trait CellularIndicatorContext {
    fn state(&mut self) -> CellularState;
    fn set_state(&mut self, state: CellularState);
}

impl CellularIndicatorContext for Context<'_, CellularIndicator> {
    #[inline]
    fn state(&mut self) -> CellularState {
        self.me().state
    }

    fn set_state(&mut self, state: CellularState) {
        if self.me().state == state {
            return;
        }
        self.me().state = state;
        let visible = state.is_visible();
        self.set_style(indicator_layout(visible));

        if visible {
            let sprite = self.me().icons.sprite(state);
            let icon_node = self.me().icon_node;
            self.at(icon_node).unwrap().set_sprite(sprite);
        }
    }
}
