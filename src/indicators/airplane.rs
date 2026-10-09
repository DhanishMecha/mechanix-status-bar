use mecha_wayland::prelude::*;

use crate::indicators::{indicator_layout, load_svg_icon};

mod svg {
    pub const AIRPLANE: &[u8] = include_bytes!("../../assets/airplane.svg");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AirplaneState {
    #[default]
    Disabled,
    Enabled,
}

impl AirplaneState {
    #[inline]
    pub fn is_visible(&self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AirplaneIcons {
    pub icon: SpriteId,
}

impl AirplaneIcons {
    pub fn load(atlas: &mut Atlas, master: u32) -> Result<Self, Error> {
        let icon = load_svg_icon(atlas, svg::AIRPLANE, master)?;
        Ok(Self { icon })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AirplaneIndicator {
    pub icons: AirplaneIcons,
    pub state: AirplaneState,
    pub icon_node: Handle<Icon>,
}

pub fn airplane_indicator(icons: AirplaneIcons) -> AirplaneIndicatorBuilder {
    AirplaneIndicatorBuilder {
        icons,
        state: AirplaneState::default(),
    }
}

pub struct AirplaneIndicatorBuilder {
    pub icons: AirplaneIcons,
    pub state: AirplaneState,
}

impl AirplaneIndicatorBuilder {
    pub fn state(mut self, state: AirplaneState) -> Self {
        self.state = state;
        self
    }
}

impl Build for AirplaneIndicatorBuilder {
    type Widget = AirplaneIndicator;
}

impl Widget for AirplaneIndicator {
    type Builder = AirplaneIndicatorBuilder;

    fn build(b: AirplaneIndicatorBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = indicator_layout(b.state.is_visible());

        let icon_node = s.spawn(me, icon(b.icons.icon).color(s.color(ColorRole::OnSurface)));

        s.on_theme(me, move |ctx| {
            let color = ctx.color(ColorRole::OnSurface);
            ctx.at(icon_node).unwrap().set_color(color);
        });

        AirplaneIndicator {
            icons: b.icons,
            state: b.state,
            icon_node,
        }
    }
}

impl AirplaneIndicator {
    #[inline]
    pub fn state(&self) -> AirplaneState {
        self.state
    }
}

pub trait AirplaneIndicatorContext {
    fn state(&mut self) -> AirplaneState;
    fn set_state(&mut self, state: AirplaneState);
}

impl AirplaneIndicatorContext for Context<'_, AirplaneIndicator> {
    #[inline]
    fn state(&mut self) -> AirplaneState {
        self.me().state
    }

    fn set_state(&mut self, state: AirplaneState) {
        if self.me().state == state {
            return;
        }
        self.me().state = state;
        self.set_style(indicator_layout(state.is_visible()));
    }
}
