use mecha_wayland::mechanix_widgets::prelude::FontBook;
use mecha_wayland::prelude::*;
use mechanix_status_bar::{
    BluetoothModule, DEFAULT_BAR_HEIGHT, DEFAULT_ICON_SIZE, GEIST_MONO_BYTES, StatusIcons,
    WifiModule, status_bar,
};

mod upower;
use upower::*;

struct Shell;
struct ShellBuilder {
    root: NodeId,
}

impl Build for ShellBuilder {
    type Widget = Shell;
}

impl Widget for Shell {
    type Builder = ShellBuilder;

    fn build(b: ShellBuilder, _me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let role = Role::Layer(LayerRole {
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: DEFAULT_BAR_HEIGHT as i32,
            namespace: "mechanix-status-bar".into(),
            keyboard_interactivity: KeyboardInteractivity::None,
        });

        let win = s.spawn_with(
            b.root,
            window()
                .title("mechanix-status-bar")
                .layout(LayoutStyle::default().height(px(DEFAULT_BAR_HEIGHT))),
            (role,),
        );

        s.spawn(win, status_bar());
        Shell
    }
}

fn main() {
    let mut app = App::new();
    app.add_module(LayoutModule)
        .add_module(PaintModule)
        .add_module(WindowModule)
        .add_module(InteractivityModule)
        .add_module(RenderModule::default())
        .insert_resource(Atlas::new());

    app.add_module(MechanixTheme::dark());

    let font = app
        .resource_mut::<Atlas>()
        .add_font(GEIST_MONO_BYTES)
        .expect("GeistMono font loads");
    app.insert_resource(FontBook::new(font));

    let icons = StatusIcons::load(&mut app.resource_mut::<Atlas>(), DEFAULT_ICON_SIZE)
        .expect("Status bar icons load");
    app.insert_resource(icons);

    app.add_module(RingModule::default())
        .add_module(DBusModule::new().system())
        .add_module(WifiModule)
        .add_module(BluetoothModule)
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>()
                .bind::<ZwlrLayerShellV1>(),
        )
        .add_module(PresentationModule {
            app_id: "mechanix.status-bar".into(),
            budget: Budget::default(),
        });

    let root = app.root();
    app.spawn(root, ShellBuilder { root });
    // The initial UPower reply emits directly to the existing status bar.
    app.add_module(UpowerModule);
    app.run();
}
