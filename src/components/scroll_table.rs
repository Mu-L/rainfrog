use color_eyre::eyre::Result;
use ratatui::{
  buffer::Cell,
  prelude::*,
  widgets::{
    Block, ScrollDirection as RatatuiScrollDir, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
    WidgetRef,
  },
};
use symbols::scrollbar;

use super::Component;

pub enum ScrollDirection {
  Left,
  Right,
  Up,
  Down,
}

#[derive(Debug, Clone, Default)]
pub struct ScrollTable<'a> {
  child_table: Table<'a>,
  child_table_state: TableState,
  parent_area: Rect,
  block: Option<Block<'a>>,
  x_offset: u16,
  max_x_offset: u16,
}

impl<'a> ScrollTable<'a> {
  pub fn new() -> Self {
    Self {
      child_table: Table::default(),
      child_table_state: TableState::default(),
      parent_area: Rect::new(0, 0, 0, 0),
      block: None,
      x_offset: 0,
      max_x_offset: 0,
    }
  }

  pub fn set_child_table(&mut self, child_table: Box<dyn WidgetRef>) -> &mut Self {
    self
  }

  pub fn block(&mut self, block: Block<'a>) -> &mut Self {
    self.block = Some(block);
    self
  }

  pub fn scroll(&mut self, direction: ScrollDirection) -> &mut Self {
    match direction {
      ScrollDirection::Left => self.x_offset = self.x_offset.saturating_sub(1),
      ScrollDirection::Right => self.x_offset = Ord::min(self.x_offset.saturating_add(1), self.max_x_offset),
      ScrollDirection::Up => self.y_offset = self.y_offset.saturating_sub(1),
      ScrollDirection::Down => self.y_offset = Ord::min(self.y_offset.saturating_add(1), self.max_offsets.max_y_offset),
    }
    self
  }

  pub fn reset_scroll(&mut self) -> &mut Self {
    self.x_offset = 0;
    self.child_table_state = TableState::default();
    self
  }

  fn widget(&'a self) -> impl Widget + 'a {
    Renderer::new(self)
  }
}

impl<'a> Component for ScrollTable<'a> {
  fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
    self.parent_area = area;
    self.max_offsets = get_max_offsets(&self.child_buffer, &self.parent_area, &self.block);
    f.render_widget(self.widget(), area);
    let vertical_scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight).symbols(scrollbar::VERTICAL);
    let mut vertical_scrollbar_state =
      ScrollbarState::new(self.max_offsets.max_y_offset as usize).position(self.y_offset as usize);
    let horizontal_scrollbar =
      Scrollbar::new(ScrollbarOrientation::HorizontalBottom).symbols(scrollbar::HORIZONTAL).thumb_symbol("▀");
    let mut horizontal_scrollbar_state =
      ScrollbarState::new(self.max_offsets.max_x_offset as usize).position(self.x_offset as usize);
    match self.max_offsets {
      MaxOffsets { max_x_offset: 0, max_y_offset: 0 } => {},
      MaxOffsets { max_x_offset: 0, max_y_offset } => {
        f.render_stateful_widget(
          vertical_scrollbar,
          area.inner(&Margin { vertical: 1, horizontal: 0 }),
          &mut vertical_scrollbar_state,
        );
      },
      MaxOffsets { max_x_offset, max_y_offset: 0 } => {
        f.render_stateful_widget(
          horizontal_scrollbar,
          area.inner(&Margin { vertical: 0, horizontal: 1 }),
          &mut horizontal_scrollbar_state,
        );
      },
      MaxOffsets { max_x_offset, max_y_offset } => {
        f.render_stateful_widget(
          vertical_scrollbar,
          area.inner(&Margin { vertical: 1, horizontal: 0 }),
          &mut vertical_scrollbar_state,
        );
        f.render_stateful_widget(
          horizontal_scrollbar,
          area.inner(&Margin { vertical: 0, horizontal: 1 }),
          &mut horizontal_scrollbar_state,
        );
      },
    };
    Ok(())
  }
}

fn get_max_x_offset(child_buffer: &Buffer, parent_area: &Rect, parent_block: &Option<Block>) -> MaxOffsets {
  parent_block.render_ref(*parent_area, &mut child_buffer.clone());
  let render_area = parent_block.inner_if_some(*parent_area);
  if render_area.is_empty() {
    return MaxOffsets { max_x_offset: 0, max_y_offset: 0 };
  }
  let parent_width = render_area.width;
  let parent_height = render_area.height;
  let content_width = child_buffer.area.width;
  let content_height = child_buffer.area.height;
  let x_diff = content_width.saturating_sub(parent_width);
  let y_diff = content_height.saturating_sub(parent_height);

  MaxOffsets { max_x_offset: x_diff, max_y_offset: y_diff }
}

fn clamp(buf: Buffer) -> Buffer {
  let height = buf.area.height;
  let width = buf.area.width;
  log::info!("height, width: {} {}", height, width);
  let mut used_height: u16 = 0;
  let mut used_width: u16 = 0;
  for y in (0..height).rev() {
    let row = get_row(&buf.content, y, width);
    for x in (0..width).rev() {
      let cell = &row[x as usize];
      if cell.symbol() != " " {
        used_height = std::cmp::max(used_height, y.saturating_add(1));
        used_width = std::cmp::max(used_width, x.saturating_add(1));
      }
    }
  }
  let mut content: Vec<ratatui::buffer::Cell> = Vec::new();
  log::info!("used height, width: {} {}", used_height, used_width);
  for y in 0..used_height {
    let row = get_row(&buf.content, y, width);
    for x in 0..used_width {
      content.push(row[x as usize].to_owned());
    }
  }
  Buffer { area: Rect::new(0, 0, used_width, used_height), content }
}

// based on scrolling approach from tui-textarea:
// https://github.com/rhysd/tui-textarea/blob/main/src/widget.rs
pub struct Renderer<'a>(&'a ScrollTable<'a>);

impl<'a> Renderer<'a> {
  pub fn new(scrolltable: &'a ScrollTable<'a>) -> Self {
    Self(scrolltable)
  }
}

impl<'a> Widget for Renderer<'a> {
  fn render(self, area: Rect, buf: &mut Buffer) {
    let scrollable = self.0;
    scrollable.block.render_ref(area, buf);
    let render_area = scrollable.block.inner_if_some(area);
    if render_area.is_empty() {
      return;
    }
    let area = render_area.intersection(buf.area);
    let content_height = scrollable.child_buffer.area.height;
    let content_width = scrollable.child_buffer.area.width;
    let max_x = Ord::min(area.x.saturating_add(area.width), area.x.saturating_add(content_width));
    let max_y = Ord::min(area.y.saturating_add(area.height), area.y.saturating_add(content_height));
    for y in area.y..max_y {
      let content_y = y + scrollable.y_offset - area.y;
      let row = get_row(&scrollable.child_buffer.content, content_y, content_width);
      for x in area.x..max_x {
        let content_x = x + scrollable.x_offset - area.x;
        let cell = &row[content_x as usize];
        buf.get_mut(x, y).set_symbol(cell.symbol()).set_fg(cell.fg).set_bg(cell.bg).set_skip(cell.skip);
      }
    }
  }
}
