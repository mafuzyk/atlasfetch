use std::collections::HashMap;

pub type Coordinate = f64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: Coordinate,
    pub y: Coordinate,
}

impl Point {
    pub fn new(x: Coordinate, y: Coordinate) -> Self {
        Point { x, y }
    }
}

impl From<(Coordinate, Coordinate)> for Point {
    fn from((x, y): (Coordinate, Coordinate)) -> Self {
        Point { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: Coordinate,
    pub height: Coordinate,
}

impl Size {
    pub fn new(width: Coordinate, height: Coordinate) -> Self {
        Size { width, height }
    }
}

impl From<(Coordinate, Coordinate)> for Size {
    fn from((w, h): (Coordinate, Coordinate)) -> Self {
        Size { width: w, height: h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: Coordinate,
    pub y: Coordinate,
    pub width: Coordinate,
    pub height: Coordinate,
}

impl Rect {
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualRegion {
    pub id: u64,
    pub name: String,
    pub rect: Rect,
    pub z_index: i32,
}

#[derive(Debug, Clone)]
pub struct WindowEntry {
    pub id: u64,
    pub position: Point,
    pub size: Size,
    pub region_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    pub output_name: String,
    pub x: Coordinate,
    pub y: Coordinate,
    pub zoom: Coordinate,
}

impl Viewport {
    pub fn new(output_name: &str) -> Self {
        Viewport {
            output_name: output_name.to_string(),
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

#[derive(Debug)]
pub struct GlobalSpace {
    regions: HashMap<u64, VisualRegion>,
    next_region_id: u64,
    windows: HashMap<u64, WindowEntry>,
    next_window_id: u64,
    /// Z-order: front-to-back (last = topmost).
    window_order: Vec<u64>,
}

impl Default for GlobalSpace {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalSpace {
    pub fn new() -> Self {
        GlobalSpace {
            regions: HashMap::new(),
            next_region_id: 1,
            windows: HashMap::new(),
            next_window_id: 1,
            window_order: Vec::new(),
        }
    }

    // ── Regions ────────────────────────────────────────────────────

    pub fn add_region(&mut self, name: String, rect: Rect) -> u64 {
        let id = self.next_region_id;
        self.next_region_id += 1;
        self.regions.insert(
            id,
            VisualRegion {
                id,
                name,
                rect,
                z_index: 0,
            },
        );
        id
    }

    pub fn region(&self, id: u64) -> Option<&VisualRegion> {
        self.regions.get(&id)
    }

    pub fn regions(&self) -> impl Iterator<Item = &VisualRegion> {
        self.regions.values()
    }

    pub fn first_region(&self) -> Option<&VisualRegion> {
        self.regions.values().next()
    }

    // ── Windows ────────────────────────────────────────────────────

    pub fn add_window(
        &mut self,
        position: impl Into<Point>,
        size: impl Into<Size>,
        region_id: Option<u64>,
    ) -> u64 {
        let id = self.next_window_id;
        self.next_window_id += 1;
        self.windows.insert(
            id,
            WindowEntry {
                id,
                position: position.into(),
                size: size.into(),
                region_id,
            },
        );
        self.window_order.push(id);
        id
    }

    pub fn remove_window(&mut self, id: u64) -> bool {
        self.window_order.retain(|&x| x != id);
        self.windows.remove(&id).is_some()
    }

    /// Move a window to the top of the z-order (end of the render list).
    pub fn raise_window(&mut self, id: u64) {
        self.window_order.retain(|&x| x != id);
        self.window_order.push(id);
    }

    /// Iterate windows in z-order (front-to-back for rendering; first = bottom).
    pub fn ordered_windows(&self) -> impl Iterator<Item = &WindowEntry> {
        self.window_order
            .iter()
            .filter_map(move |id| self.windows.get(id))
    }

    pub fn window_position(&self, id: u64) -> Option<Point> {
        self.windows.get(&id).map(|w| w.position)
    }

    pub fn window_entry(&self, id: u64) -> Option<&WindowEntry> {
        self.windows.get(&id)
    }

    pub fn move_window(&mut self, id: u64, new_pos: impl Into<Point>) -> bool {
        self.windows
            .get_mut(&id)
            .map(|w| {
                w.position = new_pos.into();
            })
            .is_some()
    }

    pub fn resize_window(&mut self, id: u64, new_size: impl Into<Size>) -> bool {
        self.windows
            .get_mut(&id)
            .map(|w| {
                w.size = new_size.into();
            })
            .is_some()
    }

    // ── Coordinate transforms ──────────────────────────────────────

    pub fn canvas_to_screen(&self, pos: Point, viewport: &Viewport) -> Point {
        Point {
            x: (pos.x - viewport.x) * viewport.zoom,
            y: (pos.y - viewport.y) * viewport.zoom,
        }
    }

    pub fn screen_to_canvas(&self, pos: Point, viewport: &Viewport) -> Point {
        Point {
            x: pos.x / viewport.zoom + viewport.x,
            y: pos.y / viewport.zoom + viewport.y,
        }
    }

    /// Returns (global_id, screen_position, screen_size) for every window
    /// whose bounding box intersects the viewport's frustum in canvas space.
    pub fn windows_visible_in(
        &self,
        viewport: &Viewport,
        screen_size: impl Into<Size>,
    ) -> Vec<(u64, Point, Size)> {
        let screen_size = screen_size.into();
        // The canvas-space rectangle currently seen by the viewport
        let viewport_canvas = Rect {
            x: viewport.x,
            y: viewport.y,
            width: screen_size.width / viewport.zoom,
            height: screen_size.height / viewport.zoom,
        };

        let mut visible = Vec::new();
        for entry in self.ordered_windows() {
            let win_rect = Rect {
                x: entry.position.x,
                y: entry.position.y,
                width: entry.size.width,
                height: entry.size.height,
            };
            if viewport_canvas.intersects(&win_rect) {
                let screen_pos = self.canvas_to_screen(entry.position, viewport);
                let screen_size = Size {
                    width: entry.size.width * viewport.zoom,
                    height: entry.size.height * viewport.zoom,
                };
                visible.push((entry.id, screen_pos, screen_size));
            }
        }
        visible
    }

    /// Returns the canvas-space position that places a window centered
    /// inside the given region's rect.
    pub fn position_in_region(&self, region_id: u64, window_size: impl Into<Size>) -> Option<Point> {
        let region = self.regions.get(&region_id)?;
        let ws = window_size.into();
        Some(Point {
            x: region.rect.x + (region.rect.width - ws.width) / 2.0,
            y: region.rect.y + (region.rect.height - ws.height) / 2.0,
        })
    }

    /// Returns the canvas-space position at the center of the current viewport.
    pub fn viewport_center_position(&self, viewport: &Viewport, screen_size: impl Into<Size>) -> Point {
        let ss = screen_size.into();
        self.screen_to_canvas(
            Point {
                x: ss.width / 2.0,
                y: ss.height / 2.0,
            },
            viewport,
        )
    }
}
