use std::thread::JoinHandle;
use std::{path::PathBuf, sync::{Arc, mpsc}, thread};

use imgui_app::Extras ;
use imgui_app::dear_imgui_rs::{Condition, Ui, WindowFlags};
use ksmap::{
    analysis::list_assets,
    drawing::DrawOptions,
    graphics::Graphics,
    partition::{GridPartitioner, IslandsPartitioner, Partitioner},
    seed::MapSeed,
    synchronization::{SyncOptions, WorldSync},
};
use libks::{map_bin, world_ini};
use ksmap::{definitions, screen_map::ScreenMap};
use anyhow::anyhow;

use crate::screens::level_map::{PartitionState, RenderState};

pub struct State {
    can_return_to_level_list: bool,
    level_dir: PathBuf,
    status: &'static str,
    thread: Option<JoinHandle<anyhow::Result<(RenderState, PartitionState)>>>,
    rx: mpsc::Receiver<LoadMessage>,
    error: Option<anyhow::Error>,
}

pub enum Task {
    ShowLevelList,
    ShowLevelMap {
        level_dir: PathBuf,
        render_state: RenderState,
        partition_state: PartitionState,
    }
}

#[derive(PartialEq, Eq)]
enum LoadMessage {
    LoadingMap,
    LoadingIni,
    LoadingDefs,
    LoadingAssets,
    Partitioning,
    Syncing,
    Done
}

impl State {
    pub fn new(ks_dir: Option<PathBuf>, level_dir: PathBuf) -> Self {
        let can_return_to_level_list = ks_dir.is_some();
        let (tx, rx) = mpsc::channel();
        let thread = {
            let ks_dir = ks_dir.unwrap_or_else(|| level_dir.join("../.."));
            let level_dir = level_dir.clone();
            thread::spawn(|| init_render_state(tx, ks_dir, level_dir))
        };
        
        Self {
            can_return_to_level_list,
            level_dir,
            thread: Some(thread),
            rx,
            status: "Loading",
            error: None,
        }
    }
}

pub fn build_ui(ui: &Ui, ex: &mut Extras, state: &mut State) -> Option<Task> {
    let mut task = None;
    
    while let Ok(message) = state.rx.try_recv() {
        state.status = match message {
            LoadMessage::LoadingMap => "Loading Map.bin",
            LoadMessage::LoadingIni => "Loading World.ini",
            LoadMessage::LoadingDefs => "Loading object definitions",
            LoadMessage::LoadingAssets => "Loading graphics",
            LoadMessage::Partitioning => "Partitioning map",
            LoadMessage::Syncing => "Syncing objects",
            LoadMessage::Done => "Done",
        };
    }
    
    if state.thread
        .as_ref()
        .is_some_and(|thread| thread.is_finished())
    {
        let thread = state.thread.take().unwrap();
        match thread.join() {
            Ok(Ok((render_state, partition_state))) => {
                task = Some(Task::ShowLevelMap {
                    level_dir: state.level_dir.clone(),
                    render_state,
                    partition_state,
                });
            }
            Ok(Err(err)) => {
                state.error = Some(err);
            }
            Err(_) => {
                state.error = Some(anyhow!("The loading thread panicked. :("));
            }
        }
    }
    
    let (width, height) = ex.window.size();
    ui.window("Main")
        .position([0.0, 0.0], Condition::Always)
        .size([width as f32, height as f32], Condition::Always)
        .flags(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE | WindowFlags::NO_RESIZE)
    .build(|| {
        if let Some(err) = &state.error {
            ui.text(format!("Failed to load the level. Reason:"));
            error_display(ui, err);
            if state.can_return_to_level_list && ui.button("Return to level list") {
                task = Some(Task::ShowLevelList);
            }
        }
        else {
            let [width_avail, height_avail] = ui.content_region_avail();
            let [width, height] = ui.calc_text_size_with_opts(
                state.status,
                false,
                width_avail,
            );
            let x = f32::round((width_avail - width) * 0.5);
            let y = f32::round((height_avail - height) * 0.5);
            ui.set_cursor_pos([x, y]);
            ui.text(state.status);
        }
    });
    
    task
}

fn init_render_state(tx: mpsc::Sender<LoadMessage>, ks_dir: PathBuf, level_dir: PathBuf) -> anyhow::Result<(RenderState, PartitionState)> {
    let _ = tx.send(LoadMessage::LoadingMap);
    let screens = map_bin::parse_map_file(level_dir.join("Map.bin"))?;
    let screen_map = ScreenMap::new(screens);
    
    let _ = tx.send(LoadMessage::LoadingIni);
    let ini = world_ini::load_ini_from_dir(&level_dir)?;

    let _ = tx.send(LoadMessage::LoadingDefs);
    let object_defs_path = {
        let mut current_dir = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::new());
        current_dir.set_file_name("ksmap_data/object_definitions.toml");
        current_dir
    };
    let object_defs = {
        let mut defs = definitions::load_object_defs(object_defs_path)?;
        definitions::insert_custom_obj_defs(&mut defs, &ini);
        Arc::new(defs)
    };

    let _ = tx.send(LoadMessage::LoadingAssets);
    let data_dir = ks_dir.join("Data");
    let templates_dir = {
        let mut current_dir = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::new());
        current_dir.set_file_name("ksmap_data/templates");
        current_dir
    };
    let mut gfx = Graphics::new(data_dir, level_dir.clone(), templates_dir, Arc::clone(&object_defs));

    let assets = list_assets(screen_map.as_slice(), &object_defs);
    let mut warnings = Vec::new();
    gfx.load_tilesets(&assets.tilesets, &mut warnings)?;
    gfx.load_gradients(&assets.gradients, &mut warnings)?;
    gfx.load_objects(&assets.objects, &mut warnings)?;

    let _ = tx.send(LoadMessage::Partitioning);
    let mut partitions;
    let partition_state;
    if screen_map.len() < 25000 {
        let partitioner = IslandsPartitioner {
            max_size: (120, 300),
            gap: 1..=10,
            force: false,
            fallback_to_grid: true,
        };
        partitions = partitioner.partitions(&screen_map);
        partition_state = PartitionState::from_islands(partitioner, &mut partitions);
    }
    else {
        let partitioner = GridPartitioner {
            max_size: (120, 300),
            rows: None,
            cols: None,
            force: false,
        };
        partitions = partitioner.partitions(&screen_map);
        partition_state = PartitionState::from_grid(partitioner, &mut partitions);
    }

    let _ = tx.send(LoadMessage::Syncing);
    let seed = MapSeed::random();
    let sync_options = SyncOptions::default();
    let world_sync = WorldSync::new(
        seed,
        &screen_map,
        &object_defs,
        &sync_options
    );

    let render_state = RenderState {
        ini,
        object_defs,
        gfx,
        screen_map,
        seed,
        partitions,
        world_sync,
        draw_options: DrawOptions::default(),
        sync_options: SyncOptions::default(),
    };
    
    let _ = tx.send(LoadMessage::Done);
    Ok((render_state, partition_state))
}

fn error_display(ui: &Ui, err: &anyhow::Error) {
    ui.bullet();
    ui.text_wrapped(err.to_string());
    for cause in err.chain().skip(1) {
        ui.bullet();
        ui.text_wrapped(format!("caused by: {cause}"));
    }
}
