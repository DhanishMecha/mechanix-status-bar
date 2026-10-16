use mecha_wayland::mechanix_widgets::prelude::{Text, TextContextExt, text};
use mecha_wayland::prelude::{Build, ColorRole, Context, Handle, LayoutStyle, Spawner, Widget};

#[derive(Debug, Clone, Copy)]
pub struct BatteryPercentage {
    pub percentage: Option<u8>,
    pub text_node: Handle<Text>,
}

pub fn battery_percentage() -> BatteryPercentageBuilder {
    BatteryPercentageBuilder
}

pub struct BatteryPercentageBuilder;

impl Build for BatteryPercentageBuilder {
    type Widget = BatteryPercentage;
}

impl Widget for BatteryPercentage {
    type Builder = BatteryPercentageBuilder;

    fn build(_b: BatteryPercentageBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default().row().center();

        let label = percentage_label(None);
        let text_node = s.spawn(
            me,
            text(label)
                .size(14.0)
                .color(ColorRole::OnSecondaryContainer),
        );

        BatteryPercentage {
            percentage: None,
            text_node,
        }
    }
}

impl BatteryPercentage {
    #[inline]
    pub fn percentage(&self) -> Option<u8> {
        self.percentage
    }
}

pub trait BatteryPercentageContext {
    fn percentage(&mut self) -> Option<u8>;
    fn set_percentage(&mut self, pct: Option<u8>);
}

impl BatteryPercentageContext for Context<'_, BatteryPercentage> {
    #[inline]
    fn percentage(&mut self) -> Option<u8> {
        self.me().percentage
    }

    fn set_percentage(&mut self, pct: Option<u8>) {
        let pct = pct.map(|pct| pct.min(100));
        if self.me().percentage == pct {
            return;
        }
        self.me().percentage = pct;
        let label = percentage_label(pct);
        let text_node = self.me().text_node;
        self.at(text_node).unwrap().set_text(label);
    }
}

fn percentage_label(percentage: Option<u8>) -> String {
    match percentage {
        Some(pct) => format!("{pct}%"),
        None => "N/A".into(),
    }
}
