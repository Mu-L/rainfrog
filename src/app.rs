use std::{
  borrow::Borrow,
  fmt::format,
  sync::{Arc, Mutex},
};

use color_eyre::eyre::Result;
use crossterm::event::KeyEvent;
use log::log;
use ratatui::{
  layout::{Constraint, Direction, Layout},
  prelude::Rect,
  widgets::{Block, Borders, Paragraph},
  Frame,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
  action::Action,
  components::{
    data::{Data, DataComponent},
    editor::Editor,
    menu::Menu,
    Component,
  },
  config::Config,
  database::{self, DbError, DbPool, Rows},
  focus::Focus,
  tui,
};

pub struct AppState {
  pub connection_string: String,
  pub focus: Focus,
  pub data: Option<Result<Rows, DbError>>,
  pub table_buf_logged: bool,
}

pub struct Components<'a> {
  pub menu: Box<dyn Component>,
  pub editor: Box<dyn Component>,
  pub data: Box<dyn DataComponent<'a>>,
}

// // TODO: see if the into_super_ref_mut fn can be fixed
//
// impl Components {
//   pub fn to_array(&mut self) -> [&mut Box<dyn Component>; 3] {
//     [&mut self.menu, &mut self.editor, self.data.into_super_ref_mut()]
//   }
// }

pub struct App {
  pub config: Config,
  pub tick_rate: Option<f64>,
  pub frame_rate: Option<f64>,
  pub components: Components<'static>,
  pub should_quit: bool,
  pub last_tick_key_events: Vec<KeyEvent>,
  pub state: Arc<Mutex<AppState>>,
  pub pool: Option<DbPool>,
}

impl App {
  pub fn new(connection_string: String, tick_rate: Option<f64>, frame_rate: Option<f64>) -> Result<Self> {
    let focus = Focus::Editor;
    let state = Arc::new(Mutex::new(AppState { connection_string, focus, data: None, table_buf_logged: false }));
    let menu = Menu::new(Arc::clone(&state));
    let editor = Editor::new(Arc::clone(&state));
    let data = Data::new(Arc::clone(&state));
    let config = Config::new()?;
    Ok(Self {
      state: Arc::clone(&state),
      tick_rate,
      frame_rate,
      components: Components { menu: Box::new(menu), editor: Box::new(editor), data: Box::new(data) },
      should_quit: false,
      config,
      last_tick_key_events: Vec::new(),
      pool: None,
    })
  }

  pub async fn run(&mut self) -> Result<()> {
    let (action_tx, mut action_rx) = mpsc::unbounded_channel();
    let connection_url = self.state.lock().unwrap().connection_string.clone();
    let pool = database::init_pool(connection_url).await?;
    log::info!("{pool:?}");
    self.pool = Some(pool);

    let mut tui = tui::Tui::new()?.tick_rate(self.tick_rate).frame_rate(self.frame_rate);
    // tui.mouse(true);
    tui.enter()?;

    self.components.menu.register_action_handler(action_tx.clone())?;
    self.components.editor.register_action_handler(action_tx.clone())?;
    self.components.data.register_action_handler(action_tx.clone())?;

    self.components.menu.register_config_handler(self.config.clone())?;
    self.components.editor.register_config_handler(self.config.clone())?;
    self.components.data.register_config_handler(self.config.clone())?;

    self.components.menu.init(tui.size()?)?;
    self.components.editor.init(tui.size()?)?;
    self.components.data.init(tui.size()?)?;

    loop {
      if let Some(e) = tui.next().await {
        let mut event_consumed = false;
        match e {
          tui::Event::Quit => action_tx.send(Action::Quit)?,
          tui::Event::Tick => action_tx.send(Action::Tick)?,
          tui::Event::Render => action_tx.send(Action::Render)?,
          tui::Event::Resize(x, y) => action_tx.send(Action::Resize(x, y))?,
          tui::Event::Key(key) => {
            if let Some(keymap) = self.config.keybindings.get(&self.state.lock().unwrap().focus) {
              if let Some(action) = keymap.get(&vec![key]) {
                log::info!("Got action: {action:?}");
                action_tx.send(action.clone())?;
                event_consumed = true;
              } else {
                // If the key was not handled as a single key action,
                // then consider it for multi-key combinations.
                self.last_tick_key_events.push(key);

                // Check for multi-key combinations
                if let Some(action) = keymap.get(&self.last_tick_key_events) {
                  log::info!("Got action: {action:?}");
                  action_tx.send(action.clone())?;
                  event_consumed = true;
                }
              }
            };
          },
          _ => {},
        }
        if !event_consumed {
          if let Some(action) = self.components.menu.handle_events(Some(e.clone()))? {
            action_tx.send(action)?;
          }
          if let Some(action) = self.components.editor.handle_events(Some(e.clone()))? {
            action_tx.send(action)?;
          }
          if let Some(action) = self.components.data.handle_events(Some(e.clone()))? {
            action_tx.send(action)?;
          }
        }
      }

      while let Ok(action) = action_rx.try_recv() {
        if action != Action::Tick && action != Action::Render {
          log::debug!("{action:?}");
        }
        let action_consumed = false;
        match &action {
          Action::Tick => {
            self.last_tick_key_events.drain(..);
          },
          Action::Quit => self.should_quit = true,
          Action::Resize(w, h) => {
            tui.resize(Rect::new(0, 0, *w, *h))?;
            tui.draw(|f| {
              self.draw_layout(f);
            })?;
          },
          Action::Render => {
            tui.draw(|f| {
              self.draw_layout(f);
            })?;
          },
          Action::FocusMenu => {
            log::info!("FocusMenu");
            let mut state = self.state.lock().unwrap();
            state.focus = Focus::Menu;
          },
          Action::FocusEditor => {
            log::info!("FocusEditor");
            let mut state = self.state.lock().unwrap();
            state.focus = Focus::Editor;
          },
          Action::FocusData => {
            log::info!("FocusData");
            let mut state = self.state.lock().unwrap();
            state.focus = Focus::Data;
          },
          Action::Query(query) => {
            log::info!("Query: {}", query.clone());
            if let Some(pool) = &self.pool {
              let results = database::query(query.clone(), pool).await;
              let mut state = self.state.lock().unwrap();
              match &results {
                Ok(rows) => {
                  log::info!("{:?}  rows", rows.len());
                  state.table_buf_logged = false;
                },
                Err(e) => {
                  log::error!("{e:?}");
                },
              };
              self.components.data.set_data_state(Some(results));
            }
          },
          _ => {},
        }
        if !action_consumed {
          if let Some(action) = self.components.menu.update(action.clone())? {
            action_tx.send(action)?;
          }
          if let Some(action) = self.components.editor.update(action.clone())? {
            action_tx.send(action)?;
          }
          if let Some(action) = self.components.data.update(action.clone())? {
            action_tx.send(action)?;
          }
        }
      }
      if self.should_quit {
        tui.stop()?;
        break;
      }
    }
    tui.exit()?;
    Ok(())
  }

  fn draw_layout(&mut self, f: &mut Frame) {
    let root_layout = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
      .split(f.size());
    let right_layout = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
      .split(root_layout[1]);

    self.components.menu.draw(f, root_layout[0]).unwrap();
    self.components.editor.draw(f, right_layout[0]).unwrap();
    self.components.data.draw(f, right_layout[1]).unwrap();
  }
}
