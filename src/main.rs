mod auth;
mod backups;
mod bisect;
mod cli;
mod config;
mod content_meta;
mod crash;
mod export;
mod fsutil;
mod groups;
mod icons;
mod import;
mod instance;
mod keymap;
mod java;
mod launch;
mod loaders;
mod migrate;
mod mod_config;
mod modrinth;
mod mod_sets;
mod mojang;
mod net;
mod paths;
mod saves;
mod shader_config;
mod shares;
mod skins;
mod store;
mod world_sets;
mod task;
mod design;
mod vanilla_options;
mod ui;

fn main() -> eframe::Result {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        let help = matches!(args.first().map(String::as_str), Some("-h" | "--help" | "help"));
        if !help {
            match migrate::run() {
                Ok(migrate::Outcome::Moved) => eprintln!("Moved Omacraft's instances and settings over to Asteria."),
                Ok(migrate::Outcome::OmacraftRunning) => {
                    eprintln!("error: Omacraft is running. Quit it so Asteria can move its instances over, then try again.");
                    std::process::exit(1);
                }
                Ok(migrate::Outcome::Nothing) => {}
                Err(e) => {
                    eprintln!("error: {e:#}");
                    std::process::exit(1);
                }
            }
        }
        std::process::exit(rt.block_on(cli::run(args)));
    }

    // wgpu prefers the discrete GPU, which on a hybrid laptop stays awake as long as
    // the launcher is open. The game picks its own GPU (see the wrapper command), and
    // WGPU_POWER_PREF still overrides this.
    let mut wgpu_options = eframe::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
        setup.power_preference = eframe::wgpu::PowerPreference::from_env().unwrap_or(eframe::wgpu::PowerPreference::LowPower);
    }
    let options = eframe::NativeOptions {
        wgpu_options,
        viewport: egui::ViewportBuilder::default()
            .with_app_id(paths::APP_ID)
            .with_title("Asteria")
            .with_inner_size([1100.0, 720.0])
            // Decorated, so other desktops give it a title bar: Hyprland asks for server-side
            // decorations and draws none, so there the window has no title bar.
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    let handle = rt.handle().clone();
    eframe::run_native(
        "Asteria",
        options,
        Box::new(move |cc| {
            let mut root = Root { rt: handle, app: None, waiting: None, next_try: std::time::Instant::now(), styled: false };
            root.start(&cc.egui_ctx);
            Ok(Box::new(root))
        }),
    )
}

/// The window: the app, or, while Omacraft's files can't be moved over yet (it's running),
/// a page saying so. The app isn't started meanwhile, so no `asteria` folder appears that
/// would stop the move for good.
struct Root {
    rt: tokio::runtime::Handle,
    app: Option<Box<ui::App>>,
    waiting: Option<String>,
    next_try: std::time::Instant,
    /// The waiting page's fonts and style are set.
    styled: bool,
}

impl Root {
    fn start(&mut self, ctx: &egui::Context) {
        match migrate::run() {
            Ok(migrate::Outcome::OmacraftRunning) => {
                self.waiting = Some("Omacraft is running. Quit it so Asteria can move your instances over; Asteria carries on by itself once it has.".into());
            }
            Ok(outcome) => {
                let mut app = ui::App::new(ctx, self.rt.clone());
                if outcome == migrate::Outcome::Moved {
                    app.toast(ui::ToastKind::Info, "Moved your instances and settings over from Omacraft. If you still run Omacraft, it now uses the same files.");
                }
                self.app = Some(Box::new(app));
                self.waiting = None;
            }
            Err(e) => self.waiting = Some(format!("Couldn't move Omacraft's files over: {e:#}")),
        }
        if self.app.is_none() && !self.styled {
            // The waiting page draws with the kit: its fonts and style, once.
            design::fonts::FontSet::resolve().apply(ctx);
            design::style::apply(ctx, &design::tokens::Tokens::new(design::system::preference().dark));
            self.styled = true;
        }
        self.next_try = std::time::Instant::now() + std::time::Duration::from_secs(2);
    }
}

impl eframe::App for Root {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.app {
            return eframe::App::ui(app.as_mut(), ui, frame);
        }
        let ctx = ui.ctx().clone();
        if std::time::Instant::now() >= self.next_try {
            self.start(&ctx);
            if self.app.is_some() {
                return;
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
        let t = design::tokens::Tokens::new(design::system::preference().dark);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(t.background)).show(ui, |ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.horizontal(|ui| {
                ui.add_space(design::tokens::GUTTER * 2.0);
                ui.vertical(|ui| {
                    ui.set_max_width(design::tokens::SETTING_DESCRIPTION_WIDTH);
                    design::kit::heading(ui, &t, "Waiting for Omacraft");
                    ui.add_space(design::tokens::UNIT);
                    design::kit::body(ui, &t, self.waiting.as_deref().unwrap_or_default());
                    ui.add_space(design::tokens::GUTTER);
                    if design::kit::Button::new("try again").show(ui, &t).clicked() {
                        self.start(&ctx);
                    }
                });
            });
        });
    }
}
