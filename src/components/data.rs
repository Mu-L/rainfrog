use std::{
  borrow::BorrowMut,
  cell::RefCell,
  collections::HashMap,
  sync::{Arc, Mutex},
  time::Duration,
};

use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{prelude::*, widgets::*};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use super::Frame;
use crate::{
  action::Action,
  app::{App, AppState},
  components::{
    scrollable::{ScrollDirection, Scrollable},
    Component,
  },
  config::{Config, KeyBindings},
  database::{get_headers, parse_value, row_to_json, row_to_vec, DbError, Rows},
  focus::Focus,
  tui::Event,
};

pub enum DataState<'a> {
  NoResults,
  Blank,
  HasResults(Table<'a>, u16, u16),
  Error(DbError),
}

pub trait SettableDataTable<'a> {
  fn set_data_state(&mut self, data: Option<Result<Rows, DbError>>);
}

pub trait DataComponent<'a>: Component + SettableDataTable<'a> {}
impl<'a, T> DataComponent<'a> for T where T: Component + SettableDataTable<'a>
{
}

pub struct Data<'a> {
  command_tx: Option<UnboundedSender<Action>>,
  config: Config,
  scrollable: Scrollable<'a>,
  data_state: DataState<'a>,
  table_state: RefCell<TableState>,
  state: Arc<Mutex<AppState>>,
}

impl<'a> Data<'a> {
  pub fn new(state: Arc<Mutex<AppState>>) -> Self {
    Data {
      command_tx: None,
      config: Config::default(),
      scrollable: Scrollable::default(),
      data_state: DataState::Blank,
      table_state: RefCell::new(TableState::default()),
      state,
    }
  }

  pub fn update_child_after_scroll(&mut self) {
    if let DataState::HasResults(table, requested_width, requested_height) = &self.data_state {
      let offset = self.scrollable.get_requested_child_offset();
      self.table_state = RefCell::new(TableState::default().with_offset(offset as usize));
      self.scrollable.child(*requested_width, *requested_height, |area, buf| {
        log::info!("updated table offset: {}", offset);
        ratatui::widgets::StatefulWidgetRef::render_ref(table, area, buf, &mut *self.table_state.borrow_mut())
      });
      self.scrollable.log();
    }
  }
}

impl<'a> SettableDataTable<'a> for Data<'a> {
  fn set_data_state(&mut self, data: Option<Result<Rows, DbError>>) {
    match data {
      Some(Ok(rows)) => {
        if rows.is_empty() {
          self.data_state = DataState::NoResults;
        } else {
          let headers = get_headers(&rows);
          let header_row =
            Row::new(headers.iter().map(|h| Cell::from(format!("{}\n{}", h.name, h.type_name))).collect::<Vec<Cell>>())
              .height(2)
              .bottom_margin(1);
          let value_rows = rows.iter().map(|r| Row::new(row_to_vec(r)).bottom_margin(1)).collect::<Vec<Row>>();
          let buf_table =
            Table::default().rows(value_rows).header(header_row).style(Style::default()).column_spacing(1);
          self.data_state = DataState::HasResults(
            buf_table,
            40_u16.saturating_mul(headers.len() as u16),
            2_u16.saturating_mul(rows.len() as u16),
          );
          if let DataState::HasResults(table, requested_width, requested_height) = &self.data_state {
            self.scrollable.child(*requested_width, *requested_height, |area, buf| {
              ratatui::widgets::StatefulWidgetRef::render_ref(table, area, buf, &mut *self.table_state.borrow_mut())
            });
          }
        }
      },
      Some(Err(e)) => {
        self.data_state = DataState::Error(e);
      },
      _ => {
        self.data_state = DataState::Blank;
      },
    }
  }
}

impl<'a> Component for Data<'a> {
  fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
    self.command_tx = Some(tx);
    Ok(())
  }

  fn register_config_handler(&mut self, config: Config) -> Result<()> {
    self.config = config;
    Ok(())
  }

  fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
    if self.state.lock().unwrap().focus != Focus::Data {
      return Ok(None);
    }
    if let Some(Event::Key(key)) = event {
      match key.code {
        KeyCode::Right => {
          self.scrollable.scroll(ScrollDirection::Right);
        },
        KeyCode::Left => {
          self.scrollable.scroll(ScrollDirection::Left);
        },
        KeyCode::Down => {
          self.scrollable.scroll(ScrollDirection::Down);
          self.update_child_after_scroll();
        },
        KeyCode::Up => {
          self.scrollable.scroll(ScrollDirection::Up);
          self.update_child_after_scroll();
        },
        _ => {},
      }
    };
    Ok(None)
  }

  fn update(&mut self, action: Action) -> Result<Option<Action>> {
    if let Action::Query(query) = action {
      self.scrollable.reset_scroll();
    }
    Ok(None)
  }

  fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
    let mut state = self.state.lock().unwrap();
    let focused = state.focus == Focus::Data;

    let block = Block::default().title("bottom").borders(Borders::ALL).border_style(if focused {
      Style::new().green()
    } else {
      Style::new().dim()
    });

    match &self.data_state {
      DataState::NoResults => {
        f.render_widget(Paragraph::new("no results").wrap(Wrap { trim: false }).block(block), area);
      },
      DataState::Blank => {
        f.render_widget(Paragraph::new("").wrap(Wrap { trim: false }).block(block), area);
      },
      DataState::HasResults(table, requested_width, requested_height) => {
        if !state.table_buf_logged {
          self.scrollable.log();
          state.table_buf_logged = true;
        }
        self.scrollable.block(block);
        self.scrollable.draw(f, area)?;
      },
      DataState::Error(e) => {
        f.render_widget(Paragraph::new(format!("{:?}", e.to_string())).wrap(Wrap { trim: false }).block(block), area);
      },
    }

    Ok(())
  }
}

// // TODO: see if this trait can be fixed and used in to_array()
//
// // based on: https://users.rust-lang.org/t/casting-traitobject-to-super-trait/33524/9
// pub trait IntoComponent<'a, Super: ?Sized> {
//   fn as_super(&self) -> &Super;
//   fn as_super_mut(&mut self) -> &mut Super;
//   fn into_super(self: Box<Self>) -> Box<Super>;
//   fn into_super_ref_mut(self: &'a mut Box<Self>) -> &'a mut Box<Super>;
// }
//
// impl<'a, T: 'a + Component> IntoComponent<'a, dyn Component + 'a> for T
// where
//   T: Component + 'a,
// {
//   fn as_super(&self) -> &(dyn Component + 'a) {
//     self
//   }
//
//   fn as_super_mut(&mut self) -> &mut (dyn Component + 'a) {
//     self
//   }
//
//   fn into_super(self: Box<Self>) -> Box<dyn Component + 'a> {
//     self
//   }
//
//   fn into_super_ref_mut(self: &'a mut Box<Self>) -> &'a mut Box<dyn Component + 'a> {
//     self
//   }
// }
//
