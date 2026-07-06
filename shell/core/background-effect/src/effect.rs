use std::{
    cell::Cell,
    collections::{HashMap, HashSet, hash_map::Entry},
    env,
    error::Error,
    ffi::c_void,
    fmt::Write as _,
    sync::OnceLock,
    time::{Duration, Instant},
};

use gio::prelude::ListModelExt;
use gtk::{
    gdk, gio,
    glib::{self, signal::SignalHandlerId, translate::ToGlibPtr},
    prelude::{Cast, DisplayExtManual, IsA, NativeExt, ObjectExt, SurfaceExt, WidgetExt},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::{Backend, ObjectId},
    delegate_noop,
    globals::{BindError, GlobalList, GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_region, wl_registry, wl_surface},
};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};

use crate::{
    BackgroundEffect, BackgroundEffectRegion,
    region::{RegionRectangle, RegionShape, RegionSize, append_region_rectangles},
};

const WINDOW_DATA_KEY: &str = "gtk4-background-effect";
const TRACE_ENV: &str = "GTK4_BACKGROUND_EFFECT_TRACE";
const LEGACY_TRACE_ENV: &str = "SHELL_CORE_BACKGROUND_EFFECT_TRACE";

unsafe extern "C" {
    fn gdk_wayland_display_get_wl_display(display: *mut gdk::ffi::GdkDisplay) -> *mut c_void;
    fn gdk_wayland_surface_get_wl_surface(surface: *mut gdk::ffi::GdkSurface) -> *mut c_void;
    fn gdk_wayland_surface_force_next_commit(surface: *mut gdk::ffi::GdkSurface);
}

/// Apply a compositor background effect to a GTK window.
///
/// This is a Wayland-only helper. It no-ops when GTK is not using the Wayland
/// backend, when the compositor does not advertise `ext-background-effect-v1`,
/// or when the window does not have a realized surface yet.
pub fn apply_background_effect(
    window: &impl IsA<gtk::Window>,
    background_effect: BackgroundEffect,
) {
    match background_effect {
        BackgroundEffect::None => {}
        BackgroundEffect::Blur(region) => enable_background_blur(window.as_ref(), region),
    }
}

fn enable_background_blur(window: &gtk::Window, region: BackgroundEffectRegion) {
    window.connect_map(move |window| {
        if let Err(error) = install_background_blur(window, region) {
            eprintln!("[gtk4-background-effect] failed to enable blur: {error}");
        }
    });
    window.connect_unrealize(|window| unsafe {
        window.steal_data::<BackgroundEffectHandle>(WINDOW_DATA_KEY);
    });

    if window.surface().is_some()
        && let Err(error) = install_background_blur(window, region)
    {
        eprintln!("[gtk4-background-effect] failed to enable blur: {error}");
    }
}

fn install_background_blur(
    window: &impl IsA<gtk::Window>,
    region: BackgroundEffectRegion,
) -> Result<(), Box<dyn Error>> {
    let window = window.as_ref();
    if unsafe {
        window
            .data::<BackgroundEffectHandle>(WINDOW_DATA_KEY)
            .is_some()
    } {
        return Ok(());
    }

    let display = window.display();
    if !display.backend().is_wayland() {
        return Ok(());
    }

    let Some(gdk_surface) = window.surface() else {
        return Ok(());
    };

    let wl_display = wayland_display(&display);
    let wl_surface = wayland_surface(&gdk_surface);
    if wl_display.is_null() || wl_surface.is_null() {
        return Ok(());
    }

    let backend = unsafe { Backend::from_foreign_display(wl_display.cast()) };
    let connection = Connection::from_backend(backend);
    let (globals, event_queue) = registry_queue_init::<BackgroundEffectState>(&connection)?;
    let queue_handle = event_queue.handle();

    let manager = match globals.bind::<ExtBackgroundEffectManagerV1, _, _>(&queue_handle, 1..=1, ())
    {
        Ok(manager) => manager,
        Err(BindError::NotPresent) => return Ok(()),
        Err(error) => return Err(Box::new(error)),
    };
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let surface_id =
        unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), wl_surface.cast()) }?;
    let wl_surface = wl_surface::WlSurface::from_id(&connection, surface_id)?;
    let effect_surface = manager.get_background_effect(&wl_surface, &queue_handle, ());

    let mut handle = BackgroundEffectHandle::new(
        connection,
        event_queue,
        globals,
        manager,
        compositor,
        effect_surface,
        region,
    );
    handle.update_blur_region(window, &gdk_surface)?;
    unsafe {
        window.set_data(WINDOW_DATA_KEY, handle);
    }
    install_dynamic_region_refresh(window, region);

    Ok(())
}

fn wayland_display(display: &gdk::Display) -> *mut c_void {
    unsafe { gdk_wayland_display_get_wl_display(display.to_glib_none().0) }
}

fn wayland_surface(surface: &gdk::Surface) -> *mut c_void {
    unsafe { gdk_wayland_surface_get_wl_surface(surface.to_glib_none().0) }
}

fn install_dynamic_region_refresh(window: &gtk::Window, region: BackgroundEffectRegion) {
    if !region.needs_layout_refresh() {
        return;
    }

    unsafe {
        let Some(mut handle) = window.data::<BackgroundEffectHandle>(WINDOW_DATA_KEY) else {
            return;
        };
        let handle = handle.as_mut();
        if region != BackgroundEffectRegion::Surface {
            handle.layout_refresh = Some(LayoutRefresh::new(window, region));
        }
        handle.frame_clock_refresh = FrameClockRefresh::new(window);
    }
    queue_installed_blur_region_refresh(window);
}

fn queue_installed_blur_region_refresh(window: &gtk::Window) {
    if request_frame_clock_refresh(window) {
        return;
    }

    let should_queue = unsafe {
        let Some(mut handle) = window.data::<BackgroundEffectHandle>(WINDOW_DATA_KEY) else {
            return;
        };
        let handle = handle.as_mut();
        if handle.refresh_pending {
            false
        } else {
            handle.refresh_pending = true;
            true
        }
    };
    if !should_queue {
        return;
    }

    let window = window.downgrade();
    glib::idle_add_local_once(move || {
        let Some(window) = window.upgrade() else {
            return;
        };
        if let Err(error) = refresh_installed_blur_region(&window) {
            eprintln!("[gtk4-background-effect] failed to refresh blur region: {error}");
        }
    });
}

fn request_frame_clock_refresh(window: &gtk::Window) -> bool {
    unsafe {
        let Some(mut handle) = window.data::<BackgroundEffectHandle>(WINDOW_DATA_KEY) else {
            return false;
        };
        let handle = handle.as_mut();
        if handle.frame_clock_refresh.is_none() {
            handle.frame_clock_refresh = FrameClockRefresh::new(window);
        }
        let Some(frame_clock) = handle
            .frame_clock_refresh
            .as_ref()
            .map(|refresh| refresh.frame_clock.clone())
        else {
            return false;
        };
        handle.frame_clock_refresh_active = true;
        frame_clock.request_phase(gdk::FrameClockPhase::LAYOUT);
        true
    }
}

fn frame_clock_refresh_is_active(window: &gtk::Window) -> bool {
    unsafe {
        window
            .data::<BackgroundEffectHandle>(WINDOW_DATA_KEY)
            .is_some_and(|handle| handle.as_ref().frame_clock_refresh_active)
    }
}

fn clear_frame_clock_refresh_active(window: &gtk::Window) {
    unsafe {
        if let Some(mut handle) = window.data::<BackgroundEffectHandle>(WINDOW_DATA_KEY) {
            handle.as_mut().frame_clock_refresh_active = false;
        }
    }
}

fn queue_layout_tree_refresh(window: &gtk::Window) {
    unsafe {
        if let Some(mut handle) = window.data::<BackgroundEffectHandle>(WINDOW_DATA_KEY) {
            handle.as_mut().layout_refresh_dirty = true;
        }
    }
    queue_installed_blur_region_refresh(window);
}

fn refresh_installed_blur_region(window: &gtk::Window) -> Result<bool, Box<dyn Error>> {
    let _span = tracing::trace_span!("background_effect.refresh_blur_region").entered();
    let Some(gdk_surface) = window.surface() else {
        clear_refresh_pending(window);
        return Ok(false);
    };

    unsafe {
        let Some(mut handle) = window.data::<BackgroundEffectHandle>(WINDOW_DATA_KEY) else {
            return Ok(false);
        };
        let handle = handle.as_mut();
        handle.refresh_pending = false;
        if handle.region.needs_layout_refresh() && handle.layout_refresh_dirty {
            handle.layout_refresh = Some(LayoutRefresh::new(window, handle.region));
            handle.layout_refresh_dirty = false;
        }
        handle.update_blur_region(window, &gdk_surface)
    }
}

fn clear_refresh_pending(window: &gtk::Window) {
    unsafe {
        if let Some(mut handle) = window.data::<BackgroundEffectHandle>(WINDOW_DATA_KEY) {
            handle.as_mut().refresh_pending = false;
        }
    }
}

fn blur_region_rectangles(
    window: &gtk::Window,
    surface: &gdk::Surface,
    region: BackgroundEffectRegion,
    geometry_cache: &mut RegionGeometryCache,
) -> RegionBuildResult {
    let surface_size = RegionSize {
        width: surface.width().max(window.width()).max(1),
        height: surface.height().max(window.height()).max(1),
    };

    let mut rectangles = Vec::new();
    let mut used_shapes = HashSet::new();
    let mut stats = RegionBuildStats::default();
    append_blur_region_rectangles(
        window,
        surface_size,
        region,
        geometry_cache,
        &mut used_shapes,
        &mut stats,
        &mut rectangles,
    );
    geometry_cache
        .local_shapes
        .retain(|key, _| used_shapes.contains(key));

    RegionBuildResult { rectangles, stats }
}

fn append_blur_region_rectangles(
    window: &gtk::Window,
    surface_size: RegionSize,
    region: BackgroundEffectRegion,
    geometry_cache: &mut RegionGeometryCache,
    used_shapes: &mut HashSet<RegionShapeCacheKey>,
    stats: &mut RegionBuildStats,
    rectangles: &mut Vec<RegionRectangle>,
) {
    match region {
        BackgroundEffectRegion::Surface => {
            rectangles.push(RegionRectangle {
                x: 0,
                y: 0,
                width: surface_size.width,
                height: surface_size.height,
            });
        }
        BackgroundEffectRegion::CssClasses(classes) => {
            collect_blur_region_rectangles_for_css_classes(
                window,
                surface_size,
                classes,
                RegionShape::Rectangle,
                geometry_cache,
                used_shapes,
                stats,
                rectangles,
            )
        }
        BackgroundEffectRegion::RoundedCssClasses { classes, radius } => {
            collect_blur_region_rectangles_for_css_classes(
                window,
                surface_size,
                classes,
                RegionShape::Rounded {
                    radius,
                    inset: 0,
                    corner_guard: 0,
                },
                geometry_cache,
                used_shapes,
                stats,
                rectangles,
            )
        }
        BackgroundEffectRegion::CornerGuardRoundedCssClasses {
            classes,
            radius,
            corner_guard,
        } => collect_blur_region_rectangles_for_css_classes(
            window,
            surface_size,
            classes,
            RegionShape::Rounded {
                radius,
                inset: 0,
                corner_guard,
            },
            geometry_cache,
            used_shapes,
            stats,
            rectangles,
        ),
        BackgroundEffectRegion::InsetRoundedCssClasses {
            classes,
            radius,
            inset,
        } => collect_blur_region_rectangles_for_css_classes(
            window,
            surface_size,
            classes,
            RegionShape::Rounded {
                radius,
                inset,
                corner_guard: 0,
            },
            geometry_cache,
            used_shapes,
            stats,
            rectangles,
        ),
        BackgroundEffectRegion::Regions(regions) => {
            for region in regions {
                append_blur_region_rectangles(
                    window,
                    surface_size,
                    *region,
                    geometry_cache,
                    used_shapes,
                    stats,
                    rectangles,
                );
            }
        }
    }
}

fn collect_blur_region_rectangles_for_css_classes(
    window: &gtk::Window,
    surface_size: RegionSize,
    classes: &[&str],
    shape: RegionShape,
    geometry_cache: &mut RegionGeometryCache,
    used_shapes: &mut HashSet<RegionShapeCacheKey>,
    stats: &mut RegionBuildStats,
    rectangles: &mut Vec<RegionRectangle>,
) {
    let root = window.upcast_ref::<gtk::Widget>();
    collect_css_class_rectangles(
        root,
        root,
        surface_size,
        classes,
        shape,
        geometry_cache,
        used_shapes,
        stats,
        rectangles,
    );
}

fn collect_css_class_rectangles(
    widget: &gtk::Widget,
    root: &gtk::Widget,
    surface_size: RegionSize,
    classes: &[&str],
    shape: RegionShape,
    geometry_cache: &mut RegionGeometryCache,
    used_shapes: &mut HashSet<RegionShapeCacheKey>,
    stats: &mut RegionBuildStats,
    rectangles: &mut Vec<RegionRectangle>,
) {
    if widget.is_drawable()
        && classes
            .iter()
            .any(|css_class| widget.has_css_class(css_class))
        && let Some(bounds) = widget.compute_bounds(root)
        && let Some(bounds) = WidgetRegionBounds::from_bounds(&bounds)
    {
        append_cached_widget_region_rectangles(
            bounds,
            surface_size,
            shape,
            geometry_cache,
            used_shapes,
            stats,
            rectangles,
        );
    }

    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        collect_css_class_rectangles(
            &widget,
            root,
            surface_size,
            classes,
            shape,
            geometry_cache,
            used_shapes,
            stats,
            rectangles,
        );
    }
}

fn append_cached_widget_region_rectangles(
    bounds: WidgetRegionBounds,
    surface_size: RegionSize,
    shape: RegionShape,
    geometry_cache: &mut RegionGeometryCache,
    used_shapes: &mut HashSet<RegionShapeCacheKey>,
    stats: &mut RegionBuildStats,
    rectangles: &mut Vec<RegionRectangle>,
) {
    let key = RegionShapeCacheKey {
        width: bounds.width,
        height: bounds.height,
        shape,
    };
    used_shapes.insert(key);

    let local_rectangles = match geometry_cache.local_shapes.entry(key) {
        Entry::Occupied(entry) => {
            stats.shape_cache_hits += 1;
            entry.into_mut()
        }
        Entry::Vacant(entry) => {
            stats.shape_cache_misses += 1;
            entry.insert(local_region_rectangles(bounds.width, bounds.height, shape))
        }
    };

    rectangles.extend(local_rectangles.iter().filter_map(|rectangle| {
        rectangle.translated_and_clipped(bounds.x, bounds.y, surface_size)
    }));
}

fn local_region_rectangles(width: i32, height: i32, shape: RegionShape) -> Vec<RegionRectangle> {
    let mut rectangles = Vec::new();
    append_region_rectangles(
        RegionRectangle {
            x: 0,
            y: 0,
            width,
            height,
        },
        shape,
        &mut rectangles,
    );
    rectangles
}

#[derive(Debug, Clone, Copy)]
struct WidgetRegionBounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl WidgetRegionBounds {
    fn from_bounds(bounds: &gtk::graphene::Rect) -> Option<Self> {
        let x = bounds.x();
        let y = bounds.y();
        let width = bounds.width();
        let height = bounds.height();
        if !x.is_finite()
            || !y.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return None;
        }

        let left = x.floor() as i32;
        let top = y.floor() as i32;
        let right = (x + width).ceil() as i32;
        let bottom = (y + height).ceil() as i32;
        if right <= left || bottom <= top {
            return None;
        }

        Some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

#[derive(Default)]
struct RegionGeometryCache {
    local_shapes: HashMap<RegionShapeCacheKey, Vec<RegionRectangle>>,
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
struct RegionShapeCacheKey {
    width: i32,
    height: i32,
    shape: RegionShape,
}

struct RegionBuildResult {
    rectangles: Vec<RegionRectangle>,
    stats: RegionBuildStats,
}

#[derive(Clone, Copy, Default)]
struct RegionBuildStats {
    shape_cache_hits: usize,
    shape_cache_misses: usize,
}

fn force_next_surface_commit(surface: &gdk::Surface) {
    unsafe {
        gdk_wayland_surface_force_next_commit(surface.to_glib_none().0);
    }
    surface.queue_render();
}

#[derive(Default)]
struct LayoutRefresh {
    widget_signals: Vec<WidgetSignal>,
    child_models: Vec<ChildModelSignal>,
}

impl LayoutRefresh {
    fn new(window: &gtk::Window, region: BackgroundEffectRegion) -> Self {
        let mut refresh = Self::default();
        refresh.watch_widget_tree(window.upcast_ref::<gtk::Widget>(), window, region);
        refresh
    }

    fn watch_widget_tree(
        &mut self,
        widget: &gtk::Widget,
        window: &gtk::Window,
        region: BackgroundEffectRegion,
    ) {
        self.watch_widget(widget, window, region);

        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            self.watch_widget_tree(&widget, window, region);
        }
    }

    fn watch_widget(
        &mut self,
        widget: &gtk::Widget,
        window: &gtk::Window,
        region: BackgroundEffectRegion,
    ) {
        self.watch_widget_property(widget, window, "width");
        self.watch_widget_property(widget, window, "height");
        self.watch_widget_property(widget, window, "visible");
        self.watch_widget_blur_membership(widget, window, region);

        let children = widget.observe_children();
        let window_weak = window.downgrade();
        let signal_id = children.connect_items_changed(move |_, _, _, _| {
            if let Some(window) = window_weak.upgrade() {
                queue_layout_tree_refresh(&window);
            }
        });
        self.child_models.push(ChildModelSignal {
            model: children,
            signal_id: Some(signal_id),
        });
    }

    fn watch_widget_property(
        &mut self,
        widget: &gtk::Widget,
        window: &gtk::Window,
        property: &'static str,
    ) {
        let window_weak = window.downgrade();
        let signal_id = widget.connect_notify_local(Some(property), move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                queue_installed_blur_region_refresh(&window);
            }
        });
        self.widget_signals.push(WidgetSignal {
            widget: widget.downgrade(),
            signal_id: Some(signal_id),
        });
    }

    fn watch_widget_blur_membership(
        &mut self,
        widget: &gtk::Widget,
        window: &gtk::Window,
        region: BackgroundEffectRegion,
    ) {
        if !region_uses_css_classes(region) {
            return;
        }

        let matches_blur_region = Cell::new(widget_matches_region_css_classes(widget, region));
        let window_weak = window.downgrade();
        let signal_id = widget.connect_notify_local(Some("css-classes"), move |widget, _| {
            let current = widget_matches_region_css_classes(widget, region);
            if matches_blur_region.replace(current) != current
                && let Some(window) = window_weak.upgrade()
            {
                queue_installed_blur_region_refresh(&window);
            }
        });
        self.widget_signals.push(WidgetSignal {
            widget: widget.downgrade(),
            signal_id: Some(signal_id),
        });
    }
}

fn region_uses_css_classes(region: BackgroundEffectRegion) -> bool {
    match region {
        BackgroundEffectRegion::Surface => false,
        BackgroundEffectRegion::CssClasses(_)
        | BackgroundEffectRegion::RoundedCssClasses { .. }
        | BackgroundEffectRegion::CornerGuardRoundedCssClasses { .. }
        | BackgroundEffectRegion::InsetRoundedCssClasses { .. } => true,
        BackgroundEffectRegion::Regions(regions) => regions
            .iter()
            .any(|region| region_uses_css_classes(*region)),
    }
}

fn widget_matches_region_css_classes(widget: &gtk::Widget, region: BackgroundEffectRegion) -> bool {
    match region {
        BackgroundEffectRegion::Surface => false,
        BackgroundEffectRegion::CssClasses(classes)
        | BackgroundEffectRegion::RoundedCssClasses { classes, .. }
        | BackgroundEffectRegion::CornerGuardRoundedCssClasses { classes, .. }
        | BackgroundEffectRegion::InsetRoundedCssClasses { classes, .. } => classes
            .iter()
            .any(|css_class| widget.has_css_class(css_class)),
        BackgroundEffectRegion::Regions(regions) => regions
            .iter()
            .any(|region| widget_matches_region_css_classes(widget, *region)),
    }
}

struct WidgetSignal {
    widget: glib::WeakRef<gtk::Widget>,
    signal_id: Option<SignalHandlerId>,
}

impl Drop for WidgetSignal {
    fn drop(&mut self) {
        let Some(signal_id) = self.signal_id.take() else {
            return;
        };
        if let Some(widget) = self.widget.upgrade() {
            widget.disconnect(signal_id);
        }
    }
}

struct ChildModelSignal {
    model: gio::ListModel,
    signal_id: Option<SignalHandlerId>,
}

impl Drop for ChildModelSignal {
    fn drop(&mut self) {
        if let Some(signal_id) = self.signal_id.take() {
            self.model.disconnect(signal_id);
        }
    }
}

struct FrameClockRefresh {
    frame_clock: gdk::FrameClock,
    signal_id: Option<SignalHandlerId>,
}

impl FrameClockRefresh {
    fn new(window: &gtk::Window) -> Option<Self> {
        let frame_clock = window.frame_clock()?;
        let window_weak = window.downgrade();
        let signal_id = frame_clock.connect_local("layout", true, move |_| {
            if let Some(window) = window_weak.upgrade()
                && frame_clock_refresh_is_active(&window)
            {
                match refresh_installed_blur_region(&window) {
                    Ok(true) => {
                        request_frame_clock_refresh(&window);
                    }
                    Ok(false) => {
                        clear_frame_clock_refresh_active(&window);
                    }
                    Err(error) => {
                        eprintln!(
                            "[gtk4-background-effect] failed to refresh blur region: {error}"
                        );
                        clear_frame_clock_refresh_active(&window);
                    }
                }
            }
            None
        });

        Some(Self {
            frame_clock,
            signal_id: Some(signal_id),
        })
    }
}

impl Drop for FrameClockRefresh {
    fn drop(&mut self) {
        if let Some(signal_id) = self.signal_id.take() {
            self.frame_clock.disconnect(signal_id);
        }
    }
}

#[derive(Debug)]
struct BackgroundEffectState;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for BackgroundEffectState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(BackgroundEffectState: wl_compositor::WlCompositor);
delegate_noop!(BackgroundEffectState: wl_region::WlRegion);
delegate_noop!(BackgroundEffectState: ignore ExtBackgroundEffectManagerV1);
delegate_noop!(BackgroundEffectState: ExtBackgroundEffectSurfaceV1);

struct BackgroundEffectHandle {
    connection: Connection,
    event_queue: EventQueue<BackgroundEffectState>,
    globals: Option<GlobalList>,
    manager: Option<ExtBackgroundEffectManagerV1>,
    compositor: wl_compositor::WlCompositor,
    surface: Option<ExtBackgroundEffectSurfaceV1>,
    region: BackgroundEffectRegion,
    last_rectangles: Option<Vec<RegionRectangle>>,
    geometry_cache: RegionGeometryCache,
    refresh_pending: bool,
    layout_refresh: Option<LayoutRefresh>,
    layout_refresh_dirty: bool,
    frame_clock_refresh: Option<FrameClockRefresh>,
    frame_clock_refresh_active: bool,
}

impl BackgroundEffectHandle {
    fn new(
        connection: Connection,
        event_queue: EventQueue<BackgroundEffectState>,
        globals: GlobalList,
        manager: ExtBackgroundEffectManagerV1,
        compositor: wl_compositor::WlCompositor,
        surface: ExtBackgroundEffectSurfaceV1,
        region: BackgroundEffectRegion,
    ) -> Self {
        Self {
            connection,
            event_queue,
            globals: Some(globals),
            manager: Some(manager),
            compositor,
            surface: Some(surface),
            region,
            last_rectangles: None,
            geometry_cache: RegionGeometryCache::default(),
            refresh_pending: false,
            layout_refresh: None,
            layout_refresh_dirty: false,
            frame_clock_refresh: None,
            frame_clock_refresh_active: false,
        }
    }

    fn update_blur_region(
        &mut self,
        window: &gtk::Window,
        gdk_surface: &gdk::Surface,
    ) -> Result<bool, Box<dyn Error>> {
        let _span =
            tracing::trace_span!("background_effect.update_blur_region", region = ?self.region)
                .entered();
        let trace_mode = TraceMode::from_env();
        let generation_started_at = (trace_mode != TraceMode::Off).then(Instant::now);
        let build_result =
            blur_region_rectangles(window, gdk_surface, self.region, &mut self.geometry_cache);
        let generation_elapsed = generation_started_at.map(|started_at| started_at.elapsed());
        let rectangles = build_result.rectangles;
        tracing::trace!(
            rectangles = rectangles.len(),
            shape_cache_hits = build_result.stats.shape_cache_hits,
            shape_cache_misses = build_result.stats.shape_cache_misses,
            "blur region generated"
        );
        if self.last_rectangles.as_ref() == Some(&rectangles) {
            if trace_mode == TraceMode::All {
                trace_blur_region(
                    "unchanged",
                    &rectangles,
                    build_result.stats,
                    generation_elapsed,
                    None,
                );
            }
            return Ok(false);
        }

        let apply_started_at = (trace_mode != TraceMode::Off).then(Instant::now);
        let queue_handle = self.event_queue.handle();
        let region = self.compositor.create_region(&queue_handle, ());
        for rectangle in &rectangles {
            region.add(rectangle.x, rectangle.y, rectangle.width, rectangle.height);
        }

        if let Some(surface) = self.surface.as_ref() {
            surface.set_blur_region(Some(&region));
        }
        region.destroy();
        force_next_surface_commit(gdk_surface);
        self.connection.flush()?;
        let apply_elapsed = apply_started_at.map(|started_at| started_at.elapsed());
        trace_blur_region(
            "changed",
            &rectangles,
            build_result.stats,
            generation_elapsed,
            apply_elapsed,
        );
        self.last_rectangles = Some(rectangles);

        Ok(true)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TraceMode {
    Off,
    Changed,
    All,
}

impl TraceMode {
    fn from_env() -> Self {
        static TRACE_MODE: OnceLock<TraceMode> = OnceLock::new();
        *TRACE_MODE.get_or_init(Self::read_env)
    }

    fn read_env() -> Self {
        let value = env::var(TRACE_ENV).or_else(|_| env::var(LEGACY_TRACE_ENV));
        match value {
            Ok(value) if value == "all" => Self::All,
            Ok(_) => Self::Changed,
            Err(_) => Self::Off,
        }
    }
}

fn trace_blur_region(
    state: &str,
    rectangles: &[RegionRectangle],
    stats: RegionBuildStats,
    generation_elapsed: Option<Duration>,
    apply_elapsed: Option<Duration>,
) {
    let Some(generation_elapsed) = generation_elapsed else {
        return;
    };

    let area: i64 = rectangles
        .iter()
        .map(|rectangle| i64::from(rectangle.width) * i64::from(rectangle.height))
        .sum();
    let apply_us = apply_elapsed
        .map(|duration| duration.as_micros().to_string())
        .unwrap_or_else(|| "-".to_owned());
    let bounds = region_bounds(rectangles)
        .map(|bounds| {
            format!(
                "{}:{} {}x{}",
                bounds.x, bounds.y, bounds.width, bounds.height
            )
        })
        .unwrap_or_else(|| "empty".to_owned());
    let sample = rectangle_sample(rectangles);

    eprintln!(
        "[gtk4-background-effect] blur region {state}: rectangles={}, area={}px, bounds={}, sample=[{}], shape-cache={}/{}, generate={}us, apply={}us",
        rectangles.len(),
        area,
        bounds,
        sample,
        stats.shape_cache_hits,
        stats.shape_cache_misses,
        generation_elapsed.as_micros(),
        apply_us,
    );
}

fn region_bounds(rectangles: &[RegionRectangle]) -> Option<RegionRectangle> {
    let first = rectangles.first()?;
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x + first.width;
    let mut bottom = first.y + first.height;

    for rectangle in &rectangles[1..] {
        left = left.min(rectangle.x);
        top = top.min(rectangle.y);
        right = right.max(rectangle.x + rectangle.width);
        bottom = bottom.max(rectangle.y + rectangle.height);
    }

    Some(RegionRectangle {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

fn rectangle_sample(rectangles: &[RegionRectangle]) -> String {
    const MAX_SAMPLE_RECTANGLES: usize = 12;

    let mut sample = String::new();
    for (index, rectangle) in rectangles.iter().take(MAX_SAMPLE_RECTANGLES).enumerate() {
        if index > 0 {
            sample.push_str(", ");
        }
        let _ = write!(
            sample,
            "{}:{} {}x{}",
            rectangle.x, rectangle.y, rectangle.width, rectangle.height
        );
    }
    if rectangles.len() > MAX_SAMPLE_RECTANGLES {
        let _ = write!(sample, ", +{}", rectangles.len() - MAX_SAMPLE_RECTANGLES);
    }

    sample
}

impl Drop for BackgroundEffectHandle {
    fn drop(&mut self) {
        self.frame_clock_refresh = None;
        self.layout_refresh = None;

        if let Some(surface) = self.surface.take() {
            surface.destroy();
        }

        if let Some(manager) = self.manager.take() {
            manager.destroy();
        }

        if let Some(globals) = self.globals.take() {
            globals.destroy();
        }

        let _ = self.connection.flush();
    }
}
