use mecha_wayland::mechanix_widgets::prelude::{Text, TextContextExt, text};
use mecha_wayland::prelude::{Build, ColorRole, Context, Handle, LayoutStyle, Spawner, Widget};

#[derive(Debug, Clone, Copy)]
pub struct BatteryPercentage {
    pub percentage: u8,
    pub text_node: Handle<Text>,
}

pub fn battery_percentage() -> BatteryPercentageBuilder {
    BatteryPercentageBuilder { percentage: 100 }
}

pub struct BatteryPercentageBuilder {
    pub percentage: u8,
}

impl BatteryPercentageBuilder {
    pub fn percentage(mut self, pct: u8) -> Self {
        self.percentage = pct.min(100);
        self
    }
}

impl Build for BatteryPercentageBuilder {
    type Widget = BatteryPercentage;
}

impl Widget for BatteryPercentage {
    type Builder = BatteryPercentageBuilder;

    fn build(b: BatteryPercentageBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default().row().center();

        let label = format!("{}%", b.percentage);
        let text_node = s.spawn(
            me,
            text(label)
                .size(14.0)
                .color(ColorRole::OnSecondaryContainer),
        );

        BatteryPercentage {
            percentage: b.percentage,
            text_node,
        }
    }
}

impl BatteryPercentage {
    #[inline]
    pub fn percentage(&self) -> u8 {
        self.percentage
    }
}

pub trait BatteryPercentageContext {
    fn percentage(&mut self) -> u8;
    fn set_percentage(&mut self, pct: u8);
}

impl BatteryPercentageContext for Context<'_, BatteryPercentage> {
    #[inline]
    fn percentage(&mut self) -> u8 {
        self.me().percentage
    }

    fn set_percentage(&mut self, pct: u8) {
        let pct = pct.min(100);
        if self.me().percentage == pct {
            return;
        }
        self.me().percentage = pct;
        let label = format!("{pct}%");
        let text_node = self.me().text_node;
        self.at(text_node).unwrap().set_text(label);
    }
}
