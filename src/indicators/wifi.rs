use mecha_wayland::prelude::*;

use crate::indicators::{indicator_layout, load_svg_icon};

mod svg {
    pub const HIGH: &[u8] = include_bytes!("../../assets/wifi/wifi-high.svg");
    pub const MEDIUM: &[u8] = include_bytes!("../../assets/wifi/wifi-medium.svg");
    pub const LOW: &[u8] = include_bytes!("../../assets/wifi/wifi-low.svg");
    pub const NONE: &[u8] = include_bytes!("../../assets/wifi/wifi-none.svg");
    pub const SLASH: &[u8] = include_bytes!("../../assets/wifi/wifi-slash.svg");
    pub const X: &[u8] = include_bytes!("../../assets/wifi/wifi-x.svg");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WifiState {
    #[default]
    High,
    Medium,
    Low,
    None,
    Disabled,
    Disconnected,
}

impl WifiState {
    #[inline]
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WifiIcons {
    pub high: SpriteId,
    pub medium: SpriteId,
    pub low: SpriteId,
    pub none: SpriteId,
    pub slash: SpriteId,
    pub x: SpriteId,
}

impl WifiIcons {
    pub fn load(atlas: &mut Atlas, master: u32) -> Result<Self, Error> {
        Ok(Self {
            high: load_svg_icon(atlas, svg::HIGH, master)?,
            medium: load_svg_icon(atlas, svg::MEDIUM, master)?,
            low: load_svg_icon(atlas, svg::LOW, master)?,
            none: load_svg_icon(atlas, svg::NONE, master)?,
            slash: load_svg_icon(atlas, svg::SLASH, master)?,
            x: load_svg_icon(atlas, svg::X, master)?,
        })
    }

    pub fn sprite(&self, state: WifiState) -> SpriteId {
        match state {
            WifiState::High => self.high,
            WifiState::Medium => self.medium,
            WifiState::Low => self.low,
            WifiState::None => self.none,
            WifiState::Disabled => self.slash,
            WifiState::Disconnected => self.x,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WifiIndicator {
    pub icons: WifiIcons,
    pub state: WifiState,
    pub icon_node: Handle<Icon>,
}

pub fn wifi_indicator(icons: WifiIcons) -> WifiIndicatorBuilder {
    WifiIndicatorBuilder {
        icons,
        state: WifiState::default(),
    }
}

pub struct WifiIndicatorBuilder {
    pub icons: WifiIcons,
    pub state: WifiState,
}

impl WifiIndicatorBuilder {
    pub fn state(mut self, state: WifiState) -> Self {
        self.state = state;
        self
    }
}

impl Build for WifiIndicatorBuilder {
    type Widget = WifiIndicator;
}

impl Widget for WifiIndicator {
    type Builder = WifiIndicatorBuilder;

    fn build(b: WifiIndicatorBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = indicator_layout(b.state.is_visible());

        let sprite = b.icons.sprite(b.state);
        let color = s.color(ColorRole::OnSurface);
        let icon_node = s.spawn(me, icon(sprite).color(color));

        s.on_theme(me, move |ctx| {
            let color = ctx.color(ColorRole::OnSurface);
            ctx.at(icon_node).unwrap().set_color(color);
        });

        WifiIndicator {
            icons: b.icons,
            state: b.state,
            icon_node,
        }
    }
}

impl WifiIndicator {
    #[inline]
    pub fn state(&self) -> WifiState {
        self.state
    }
}

pub trait WifiIndicatorContext {
    fn state(&mut self) -> WifiState;
    fn set_state(&mut self, state: WifiState);
}

impl WifiIndicatorContext for Context<'_, WifiIndicator> {
    #[inline]
    fn state(&mut self) -> WifiState {
        self.me().state
    }

    fn set_state(&mut self, state: WifiState) {
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
