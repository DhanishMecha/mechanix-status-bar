use mecha_wayland::prelude::*;

use crate::indicators::*;
use crate::{DEFAULT_BAR_HEIGHT, StatusIcons};

#[derive(Debug, Clone, Copy)]
pub struct StatusBar {
    pub glyph: Handle<GlyphIndicator>,
    pub airplane: Handle<AirplaneIndicator>,
    pub bluetooth: Handle<BluetoothIndicator>,
    pub cellular: Handle<CellularIndicator>,
    pub wifi: Handle<WifiIndicator>,
    pub battery: Handle<BatteryPercentage>,
}

pub fn status_bar() -> StatusBarBuilder {
    StatusBarBuilder
}

pub struct StatusBarBuilder;

impl Build for StatusBarBuilder {
    type Widget = StatusBar;
}

impl Widget for StatusBar {
    type Builder = StatusBarBuilder;

    fn build(_b: StatusBarBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        // Outer status bar: height 36px, full width, padded, space-between left glyph & right cluster
        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default()
            .row()
            .center()
            .width(percent(100.0))
            .height(px(DEFAULT_BAR_HEIGHT))
            .padding(Insets::symmetric(px(20.0), px(8.0)))
            .justify(Justify::SpaceBetween);

        // Themed background
        let bg = s.color(ColorRole::SurfaceContainer);
        *s.component_mut::<Paint>(me).unwrap() = Paint::Quad(Quad::new(bg));

        s.on_theme(me, move |ctx| {
            let bg = ctx.color(ColorRole::SurfaceContainer);
            ctx.set_paint(Paint::Quad(Quad::new(bg)));
        });

        // Left cluster container: flex row with gap between items (e.g. [glyph] [clock])
        let left_cluster = s.spawn(
            me,
            div().style(LayoutStyle::default().row().center().gap(px(8.0))),
        );
        let glyph = s.spawn(left_cluster, glyph_indicator());
        // TODO: clock here

        // Right cluster container: flex row with gap between items
        let right_cluster = s.spawn(
            me,
            div().style(LayoutStyle::default().row().center().gap(px(8.0))),
        );

        let icons = *s.resource::<StatusIcons>();

        // Layout order (left-to-right in the right cluster):
        // [airplane] [bluetooth] [cellular] [wifi] [battery_percentage]
        // This places battery percentage on the rightmost side,
        // and to the left of it: wifi, cellular, bluetooth, airplane.
        let airplane = s.spawn(right_cluster, airplane_indicator(icons.airplane));
        let bluetooth = s.spawn(
            right_cluster,
            bluetooth_indicator(icons.bluetooth).state(BluetoothState::Off),
        );
        let cellular = s.spawn(right_cluster, cellular_indicator(icons.cellular));
        let wifi = s.spawn(
            right_cluster,
            wifi_indicator(icons.wifi).state(WifiState::Disabled),
        );
        let battery = s.spawn(right_cluster, battery_percentage());

        // Event handler for external/pluggable status updates (D-Bus or Demo)
        s.on::<StatusUpdate>(me, move |ctx, event| match *event {
            StatusUpdate::Wifi(state) => {
                ctx.at(wifi).unwrap().set_state(state);
            }
            StatusUpdate::Bluetooth(state) => {
                ctx.at(bluetooth).unwrap().set_state(state);
            }
            StatusUpdate::Cellular(state) => {
                ctx.at(cellular).unwrap().set_state(state);
            }
            StatusUpdate::Airplane(state) => {
                ctx.at(airplane).unwrap().set_state(state);
            }
            StatusUpdate::Battery(pct) => {
                ctx.at(battery).unwrap().set_percentage(pct);
            }
            StatusUpdate::Glyph(state) => {
                ctx.at(glyph).unwrap().set_state(state);
            }
        });

        StatusBar {
            glyph,
            airplane,
            bluetooth,
            cellular,
            wifi,
            battery,
        }
    }
}

/// A decoupled status update event for controlling status bar indicators.
/// Emitted by either background D-Bus monitors or the visual demo driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusUpdate {
    Wifi(WifiState),
    Bluetooth(BluetoothState),
    Cellular(CellularState),
    Airplane(AirplaneState),
    Battery(Option<u8>),
    Glyph(GlyphState),
}

impl Event for StatusUpdate {}
