use std::{collections::HashMap, fmt, future::Future, path::PathBuf, pin::Pin, sync::Arc};

use futures_util::{Stream, StreamExt, stream};
use rxrust::prelude::Observable as _;
use tokio::sync::OnceCell;
use zbus::{
    Connection, Message, Proxy,
    names::{OwnedBusName, OwnedInterfaceName},
    proxy::PropertyChanged,
    zvariant::{DynamicDeserialize, OwnedObjectPath, OwnedValue},
};

use super::{
    Observable, Source, defer, shared_by_key,
    support::{from_stream_result, log_errors},
};

const DBUS_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const DBUS_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

/// D-Bus bus used by a source descriptor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Bus {
    Session,
    System,
}

impl Bus {
    async fn connection(self) -> zbus::Result<Connection> {
        match self {
            Self::Session => cached_session_connection().await,
            Self::System => cached_system_connection().await,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::System => "system",
        }
    }
}

async fn cached_session_connection() -> zbus::Result<Connection> {
    static CONNECTION: OnceCell<Connection> = OnceCell::const_new();
    CONNECTION
        .get_or_try_init(Connection::session)
        .await
        .cloned()
}

async fn cached_system_connection() -> zbus::Result<Connection> {
    static CONNECTION: OnceCell<Connection> = OnceCell::const_new();
    CONNECTION
        .get_or_try_init(Connection::system)
        .await
        .cloned()
}

/// Identifies one D-Bus object/interface pair.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ObjectDescriptor {
    pub bus: Bus,
    pub destination: OwnedBusName,
    pub path: OwnedObjectPath,
    pub interface: OwnedInterfaceName,
}

impl ObjectDescriptor {
    pub fn parse(bus: Bus, destination: &str, path: &str, interface: &str) -> Result<Self, String> {
        Ok(Self {
            bus,
            destination: parse_bus_name(destination)?,
            path: parse_object_path(path)?,
            interface: parse_interface_name(interface)?,
        })
    }

    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.bus.key(),
            self.destination,
            self.path,
            self.interface
        )
    }
}

/// Identifies one D-Bus property.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PropertyDescriptor {
    pub object: ObjectDescriptor,
    pub property: String,
}

impl PropertyDescriptor {
    pub fn new(object: ObjectDescriptor, property: impl Into<String>) -> Self {
        Self {
            object,
            property: property.into(),
        }
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.object.key(), self.property)
    }
}

/// Identifies one D-Bus signal.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SignalDescriptor {
    pub object: ObjectDescriptor,
    pub signal: String,
}

impl SignalDescriptor {
    pub fn new(object: ObjectDescriptor, signal: impl Into<String>) -> Self {
        Self {
            object,
            signal: signal.into(),
        }
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.object.key(), self.signal)
    }
}

/// Identifies an object implementing `org.freedesktop.DBus.ObjectManager`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ObjectManagerDescriptor {
    pub bus: Bus,
    pub destination: OwnedBusName,
    pub path: OwnedObjectPath,
}

impl ObjectManagerDescriptor {
    pub fn parse(bus: Bus, destination: &str, path: &str) -> Result<Self, String> {
        Ok(Self {
            bus,
            destination: parse_bus_name(destination)?,
            path: parse_object_path(path)?,
        })
    }

    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.bus.key(), self.destination, self.path)
    }
}

/// One object from an ObjectManager snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct DbusObject {
    pub path: OwnedObjectPath,
    pub interfaces: Vec<DbusInterface>,
}

/// One interface and its current ObjectManager property snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct DbusInterface {
    pub name: OwnedInterfaceName,
    pub properties: Vec<DbusPropertyValue>,
}

/// One property value from an ObjectManager snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct DbusPropertyValue {
    pub name: String,
    pub value: Arc<OwnedValue>,
}

type RawManagedObjects =
    HashMap<OwnedObjectPath, HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>>;
type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<OwnedInterfaceName, HashMap<String, Arc<OwnedValue>>>>;
type BoxedMessageStream = Pin<Box<dyn Stream<Item = Message> + Send + 'static>>;
type BoxedPropertiesChangedStream =
    Pin<Box<dyn Stream<Item = Result<PropertiesChanged, String>> + Send + 'static>>;
type BoxedPropertyChangedStream<T> =
    Pin<Box<dyn Stream<Item = PropertyChanged<'static, T>> + Send + 'static>>;

#[derive(Clone, Debug)]
struct PropertiesChanged {
    interface: OwnedInterfaceName,
    changed: Arc<HashMap<String, Arc<OwnedValue>>>,
    invalidated: Arc<Vec<String>>,
}

/// Typed handle for objects discovered through `org.freedesktop.DBus.ObjectManager`.
pub trait ObjectModel: Clone + PartialEq + Send + 'static {
    const INTERFACE: &'static str;

    fn at(path: OwnedObjectPath) -> Self;
}

/// Emits a typed property from a zbus proxy built by `make_proxy`.
///
/// This is the bridge for zbus-generated proxy models: service-specific code
/// builds the proxy and shell-core turns its cached property stream into an
/// Observable. In zbus 4.x, `receive_property_changed` only emits when property
/// caching is enabled on the proxy, so callers should not use
/// `CacheProperties::No` for proxies passed here.
pub fn proxy_property<T, F, Fut>(
    key: impl Into<String>,
    property_name: &'static str,
    make_proxy: F,
) -> Observable<T>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + Sync + Unpin + 'static,
    T::Error: Into<zbus::Error>,
    F: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = zbus::Result<Proxy<'static>>> + Send + 'static,
{
    let key = key.into();
    shared_by_key("dbus-proxy-property", key.clone(), move || {
        let path = descriptor_error_path(&key);
        let observable = from_stream_result(proxy_property_stream::<T, _, _>(
            property_name,
            make_proxy.clone(),
        ))
        .distinct_until_changed()
        .box_it();
        log_errors("dbus-proxy-property", path, observable)
    })
}

/// Emits a typed D-Bus property value.
///
/// `None` means the property is absent or was invalidated and could not be read.
pub fn property<T>(descriptor: PropertyDescriptor) -> Observable<Option<T>>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: fmt::Display,
{
    let key = descriptor.key();
    shared_by_key("dbus-property", key, move || {
        let path = descriptor_error_path(&descriptor.key());
        let descriptor = descriptor.clone();
        let observable =
            defer(move || from_stream_result(property_stream::<T>(descriptor.clone())))
                .distinct_until_changed()
                .box_it();
        log_errors("dbus-property", path, observable)
    })
}

/// Emits a typed D-Bus property value, substituting `default` while absent.
pub fn property_or<T>(descriptor: PropertyDescriptor, default: T) -> Observable<T>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: fmt::Display,
{
    property(descriptor)
        .map(move |value| value.unwrap_or_else(|| default.clone()))
        .box_it()
}

/// Emits a Niri-style optional property encoded as an array with zero or one item.
///
/// This differs from `property<T>() -> Observable<Option<T>>`: `property`
/// returns `None` when the property is absent or invalidated, while this helper
/// expects a present D-Bus property whose wire value is `Vec<T>`.
pub fn optional_array_property<T>(descriptor: PropertyDescriptor) -> Observable<Option<T>>
where
    T: Clone + PartialEq + Send + 'static,
    Vec<T>: TryFrom<OwnedValue> + Clone + PartialEq,
    <Vec<T> as TryFrom<OwnedValue>>::Error: fmt::Display,
{
    property::<Vec<T>>(descriptor)
        .map(|values| values.and_then(|mut values| values.drain(..).next()))
        .distinct_until_changed()
        .box_it()
}

/// Emits an optional object model encoded as a zero-or-one object-path array.
pub fn optional_model_property<T>(descriptor: PropertyDescriptor) -> Observable<Option<T>>
where
    T: ObjectModel,
{
    optional_array_property::<OwnedObjectPath>(descriptor)
        .map(|path| path.map(T::at))
        .distinct_until_changed()
        .box_it()
}

/// Emits a typed D-Bus property value as a `Source`.
///
/// This is the Source-native bridge used by generated DBus model helpers. A
/// missing property is reported as a source error because the generated method
/// has the concrete property type in its signature.
pub fn required_property_source<T>(descriptor: PropertyDescriptor) -> Source<T>
where
    T: TryFrom<OwnedValue> + Send + 'static,
    T::Error: fmt::Display,
{
    Source::from_task(move |sender| {
        let descriptor = descriptor.clone();
        async move {
            let mut stream = Box::pin(property_stream::<T>(descriptor.clone()));
            while let Some(item) = stream.next().await {
                let result = item.and_then(|value| {
                    value.ok_or_else(|| format!("D-Bus property {} is absent", descriptor.key()))
                });
                if sender.send(result).await.is_err() {
                    return;
                }
            }
        }
    })
}

/// Emits a typed optional D-Bus property value as a `Source`.
///
/// Niri-style optional properties are exposed as arrays with zero or one item.
/// Generated DBus model helpers use this so consumers can declare
/// `Option<T>` instead of depending on that wire encoding.
pub fn optional_property_source<T>(descriptor: PropertyDescriptor) -> Source<Option<T>>
where
    T: Send + 'static,
    Vec<T>: TryFrom<OwnedValue>,
    <Vec<T> as TryFrom<OwnedValue>>::Error: fmt::Display,
{
    Source::from_task(move |sender| {
        let descriptor = descriptor.clone();
        async move {
            let mut stream = Box::pin(property_stream::<Vec<T>>(descriptor.clone()));
            while let Some(item) = stream.next().await {
                let result = item.map(|value| value.and_then(|mut values| values.drain(..).next()));
                if sender.send(result).await.is_err() {
                    return;
                }
            }
        }
    })
}

enum ProxyPropertyPhase {
    Connect,
    InitialRead,
    Watch,
    Done,
}

struct ProxyPropertyStreamState<T, F> {
    property_name: &'static str,
    make_proxy: F,
    proxy: Option<Proxy<'static>>,
    stream: Option<BoxedPropertyChangedStream<T>>,
    phase: ProxyPropertyPhase,
}

fn proxy_property_stream<T, F, Fut>(
    property_name: &'static str,
    make_proxy: F,
) -> impl Stream<Item = Result<T, String>>
where
    T: TryFrom<OwnedValue> + Send + Sync + Unpin + 'static,
    T::Error: Into<zbus::Error>,
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = zbus::Result<Proxy<'static>>> + Send + 'static,
{
    stream::unfold(
        ProxyPropertyStreamState {
            property_name,
            make_proxy,
            proxy: None,
            stream: None,
            phase: ProxyPropertyPhase::Connect,
        },
        |mut state| async move {
            loop {
                match state.phase {
                    ProxyPropertyPhase::Connect => match (state.make_proxy)().await {
                        Ok(proxy) => {
                            let stream = proxy.receive_property_changed(state.property_name).await;
                            state.proxy = Some(proxy);
                            state.stream = Some(Box::pin(stream));
                            state.phase = ProxyPropertyPhase::InitialRead;
                        }
                        Err(error) => {
                            state.phase = ProxyPropertyPhase::Done;
                            return Some((
                                Err(format_dbus_error("connect proxy property source", error)),
                                state,
                            ));
                        }
                    },
                    ProxyPropertyPhase::InitialRead => {
                        state.phase = ProxyPropertyPhase::Watch;
                        let result = state
                            .proxy()
                            .get_property(state.property_name)
                            .await
                            .map_err(|error| format_dbus_error("read proxy property", error));
                        return Some((result, state));
                    }
                    ProxyPropertyPhase::Watch => {
                        let Some(change) = state
                            .stream
                            .as_mut()
                            .expect("stream initialized")
                            .next()
                            .await
                        else {
                            return None;
                        };
                        let result = change.get().await.map_err(|error| {
                            format_dbus_error("read changed proxy property", error)
                        });
                        return Some((result, state));
                    }
                    ProxyPropertyPhase::Done => return None,
                }
            }
        },
    )
}

impl<T, F> ProxyPropertyStreamState<T, F> {
    fn proxy(&self) -> &Proxy<'static> {
        self.proxy.as_ref().expect("proxy initialized")
    }
}

/// Emits decoded payloads from one D-Bus signal.
///
/// For multi-argument signals, use a tuple as `T`.
pub fn signal<T>(descriptor: SignalDescriptor) -> Observable<T>
where
    T: for<'de> DynamicDeserialize<'de> + Clone + Send + 'static,
{
    let key = descriptor.key();
    shared_by_key("dbus-signal", key, move || {
        let path = descriptor_error_path(&descriptor.key());
        let observable = from_stream_result(signal_stream::<T>(descriptor.clone())).box_it();
        log_errors("dbus-signal", path, observable)
    })
}

/// Emits ObjectManager snapshots and membership deltas as full deterministic snapshots.
pub fn object_manager(descriptor: ObjectManagerDescriptor) -> Observable<Vec<DbusObject>> {
    let key = descriptor.key();
    shared_by_key("dbus-object-manager", key, move || {
        let path = descriptor_error_path(&descriptor.key());
        let observable = from_stream_result(object_manager_stream(descriptor.clone())).box_it();
        log_errors("dbus-object-manager", path, observable)
    })
}

/// Emits typed model handles for objects currently exposing `T::INTERFACE`.
///
/// This listens only to ObjectManager membership changes. Properties of the
/// returned objects are subscribed separately by each model property observable.
pub fn models<T>(descriptor: ObjectManagerDescriptor) -> Observable<Vec<T>>
where
    T: ObjectModel,
{
    let key = format!("{}:{}", descriptor.key(), T::INTERFACE);
    let interface = parse_interface_name(T::INTERFACE)
        .expect("ObjectModel::INTERFACE should be a valid D-Bus interface name");
    shared_by_key("dbus-models", key, move || {
        let path = descriptor_error_path(&descriptor.key());
        let observable = from_stream_result(object_model_stream::<T>(
            descriptor.clone(),
            interface.clone(),
        ))
        .distinct_until_changed()
        .box_it();
        log_errors("dbus-models", path, observable)
    })
}

enum PropertyPhase {
    Connect,
    InitialRead,
    Watch,
    Done,
}

struct PropertyStreamState {
    descriptor: PropertyDescriptor,
    proxy: Option<Proxy<'static>>,
    stream: Option<BoxedPropertiesChangedStream>,
    phase: PropertyPhase,
}

fn properties_changed(object: ObjectDescriptor) -> Observable<PropertiesChanged> {
    let key = object.key();
    shared_by_key("dbus-properties-changed", key, move || {
        let path = descriptor_error_path(&object.key());
        let object = object.clone();
        let observable =
            defer(move || from_stream_result(properties_changed_stream(object.clone()))).box_it();
        log_errors("dbus-properties-changed", path, observable)
    })
}

fn properties_changed_stream(
    object: ObjectDescriptor,
) -> impl Stream<Item = Result<PropertiesChanged, String>> {
    stream::unfold(
        PropertySignalStreamState {
            object,
            proxy: None,
            stream: None,
            phase: SignalPhase::Connect,
        },
        |mut state| async move {
            loop {
                match state.phase {
                    SignalPhase::Connect => match properties_proxy(&state.object).await {
                        Ok(proxy) => {
                            state.proxy = Some(proxy);
                            state.phase = SignalPhase::Watch;
                        }
                        Err(error) => {
                            state.phase = SignalPhase::Done;
                            return Some((
                                Err(format_dbus_error("connect PropertiesChanged source", error)),
                                state,
                            ));
                        }
                    },
                    SignalPhase::Watch => {
                        if state.stream.is_none() {
                            match state.proxy().receive_signal("PropertiesChanged").await {
                                Ok(stream) => state.stream = Some(Box::pin(stream)),
                                Err(error) => {
                                    state.phase = SignalPhase::Done;
                                    return Some((
                                        Err(format_dbus_error(
                                            "subscribe PropertiesChanged",
                                            error,
                                        )),
                                        state,
                                    ));
                                }
                            }
                        }

                        let Some(message) = state
                            .stream
                            .as_mut()
                            .expect("stream initialized")
                            .next()
                            .await
                        else {
                            return None;
                        };

                        match decode_properties_changed(message) {
                            Ok(update) => return Some((Ok(update), state)),
                            Err(error) => {
                                state.phase = SignalPhase::Done;
                                return Some((Err(error), state));
                            }
                        }
                    }
                    SignalPhase::Done => return None,
                }
            }
        },
    )
}

struct PropertySignalStreamState {
    object: ObjectDescriptor,
    proxy: Option<Proxy<'static>>,
    stream: Option<BoxedMessageStream>,
    phase: SignalPhase,
}

impl PropertySignalStreamState {
    fn proxy(&self) -> &Proxy<'static> {
        self.proxy.as_ref().expect("proxy initialized")
    }
}

fn property_stream<T>(
    descriptor: PropertyDescriptor,
) -> impl Stream<Item = Result<Option<T>, String>>
where
    T: TryFrom<OwnedValue> + Send + 'static,
    T::Error: fmt::Display,
{
    stream::unfold(
        PropertyStreamState {
            descriptor,
            proxy: None,
            stream: None,
            phase: PropertyPhase::Connect,
        },
        |mut state| async move {
            loop {
                match state.phase {
                    PropertyPhase::Connect => {
                        match properties_proxy(&state.descriptor.object).await {
                            Ok(proxy) => {
                                state.proxy = Some(proxy);
                                state.phase = PropertyPhase::InitialRead;
                            }
                            Err(error) => {
                                state.phase = PropertyPhase::Done;
                                return Some((
                                    Err(format_dbus_error("connect property source", error)),
                                    state,
                                ));
                            }
                        }
                    }
                    PropertyPhase::InitialRead => {
                        state.phase = PropertyPhase::Watch;
                        let result = read_property::<T>(&state.descriptor, state.proxy()).await;
                        return Some((result, state));
                    }
                    PropertyPhase::Watch => {
                        if state.stream.is_none() {
                            state.stream = Some(Box::pin(
                                properties_changed(state.descriptor.object.clone()).into_stream(),
                            ));
                        }

                        let Some(update) = state
                            .stream
                            .as_mut()
                            .expect("stream initialized")
                            .next()
                            .await
                        else {
                            return None;
                        };

                        let result =
                            property_changed_value::<T>(&state.descriptor, state.proxy(), update)
                                .await;
                        match result {
                            PropertyChange::Emit(value) => return Some((value, state)),
                            PropertyChange::Ignore => continue,
                            PropertyChange::Error(error) => {
                                state.phase = PropertyPhase::Done;
                                return Some((Err(error), state));
                            }
                        }
                    }
                    PropertyPhase::Done => return None,
                }
            }
        },
    )
}

impl PropertyStreamState {
    fn proxy(&self) -> &Proxy<'static> {
        self.proxy.as_ref().expect("proxy initialized")
    }
}

enum PropertyChange<T> {
    Emit(Result<Option<T>, String>),
    Ignore,
    Error(String),
}

async fn property_changed_value<T>(
    descriptor: &PropertyDescriptor,
    proxy: &Proxy<'static>,
    update: Result<PropertiesChanged, String>,
) -> PropertyChange<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: fmt::Display,
{
    let update = match update {
        Ok(update) => update,
        Err(error) => return PropertyChange::Error(error),
    };

    if update.interface != descriptor.object.interface {
        return PropertyChange::Ignore;
    }

    if let Some(value) = update.changed.get(&descriptor.property) {
        let value = match value.as_ref().try_clone() {
            Ok(value) => value,
            Err(error) => {
                return PropertyChange::Error(format!(
                    "clone D-Bus property {} failed: {error}",
                    descriptor.key()
                ));
            }
        };
        return PropertyChange::Emit(decode_property(descriptor, value).map(Some));
    }

    if update
        .invalidated
        .iter()
        .any(|property| property == &descriptor.property)
    {
        return PropertyChange::Emit(read_property(descriptor, proxy).await);
    }

    PropertyChange::Ignore
}

fn decode_properties_changed(message: Message) -> Result<PropertiesChanged, String> {
    let (interface, changed, invalidated) = message
        .body()
        .deserialize::<(OwnedInterfaceName, HashMap<String, OwnedValue>, Vec<String>)>()
        .map_err(|error| format_dbus_error("decode PropertiesChanged", error))?;

    Ok(PropertiesChanged {
        interface,
        changed: Arc::new(arc_properties(changed)),
        invalidated: Arc::new(invalidated),
    })
}

async fn read_property<T>(
    descriptor: &PropertyDescriptor,
    proxy: &Proxy<'static>,
) -> Result<Option<T>, String>
where
    T: TryFrom<OwnedValue>,
    T::Error: fmt::Display,
{
    let interface: zbus::names::InterfaceName<'_> = (&descriptor.object.interface).into();
    let body = (interface, descriptor.property.as_str());
    match proxy.call::<_, _, OwnedValue>("Get", &body).await {
        Ok(value) => decode_property(descriptor, value).map(Some),
        Err(error) if is_missing_property_error(&error) => Ok(None),
        Err(error) => Err(format_dbus_error("read property", error)),
    }
}

fn decode_property<T>(descriptor: &PropertyDescriptor, value: OwnedValue) -> Result<T, String>
where
    T: TryFrom<OwnedValue>,
    T::Error: fmt::Display,
{
    T::try_from(value)
        .map_err(|error| format!("decode D-Bus property {} failed: {error}", descriptor.key()))
}

enum SignalPhase {
    Connect,
    Watch,
    Done,
}

struct SignalStreamState {
    descriptor: SignalDescriptor,
    proxy: Option<Proxy<'static>>,
    stream: Option<BoxedMessageStream>,
    phase: SignalPhase,
}

fn signal_stream<T>(descriptor: SignalDescriptor) -> impl Stream<Item = Result<T, String>>
where
    T: for<'de> DynamicDeserialize<'de> + Send + 'static,
{
    stream::unfold(
        SignalStreamState {
            descriptor,
            proxy: None,
            stream: None,
            phase: SignalPhase::Connect,
        },
        |mut state| async move {
            loop {
                match state.phase {
                    SignalPhase::Connect => match object_proxy(&state.descriptor.object).await {
                        Ok(proxy) => {
                            state.proxy = Some(proxy);
                            state.phase = SignalPhase::Watch;
                        }
                        Err(error) => {
                            state.phase = SignalPhase::Done;
                            return Some((
                                Err(format_dbus_error("connect signal source", error)),
                                state,
                            ));
                        }
                    },
                    SignalPhase::Watch => {
                        if state.stream.is_none() {
                            let proxy = state.proxy.as_ref().expect("proxy initialized");
                            match proxy.receive_signal(state.descriptor.signal.clone()).await {
                                Ok(stream) => state.stream = Some(Box::pin(stream)),
                                Err(error) => {
                                    state.phase = SignalPhase::Done;
                                    return Some((
                                        Err(format_dbus_error("subscribe signal", error)),
                                        state,
                                    ));
                                }
                            }
                        }

                        let Some(message) = state
                            .stream
                            .as_mut()
                            .expect("stream initialized")
                            .next()
                            .await
                        else {
                            return None;
                        };

                        match message.body().deserialize::<T>() {
                            Ok(value) => return Some((Ok(value), state)),
                            Err(error) => {
                                state.phase = SignalPhase::Done;
                                return Some((
                                    Err(format_dbus_error("decode signal", error)),
                                    state,
                                ));
                            }
                        }
                    }
                    SignalPhase::Done => return None,
                }
            }
        },
    )
}

enum ObjectManagerPhase {
    Connect,
    InitialRead,
    Watch,
    Done,
}

struct ObjectManagerStreamState {
    descriptor: ObjectManagerDescriptor,
    proxy: Option<Proxy<'static>>,
    stream: Option<BoxedMessageStream>,
    objects: ManagedObjects,
    phase: ObjectManagerPhase,
}

struct ObjectModelStreamState {
    descriptor: ObjectManagerDescriptor,
    interface: OwnedInterfaceName,
    proxy: Option<Proxy<'static>>,
    stream: Option<BoxedMessageStream>,
    paths: Vec<OwnedObjectPath>,
    phase: ObjectManagerPhase,
}

fn object_manager_stream(
    descriptor: ObjectManagerDescriptor,
) -> impl Stream<Item = Result<Vec<DbusObject>, String>> {
    stream::unfold(
        ObjectManagerStreamState {
            descriptor,
            proxy: None,
            stream: None,
            objects: ManagedObjects::new(),
            phase: ObjectManagerPhase::Connect,
        },
        |mut state| async move {
            loop {
                match state.phase {
                    ObjectManagerPhase::Connect => {
                        match object_manager_proxy(&state.descriptor).await {
                            Ok(proxy) => {
                                state.proxy = Some(proxy);
                                state.phase = ObjectManagerPhase::InitialRead;
                            }
                            Err(error) => {
                                state.phase = ObjectManagerPhase::Done;
                                return Some((
                                    Err(format_dbus_error("connect ObjectManager source", error)),
                                    state,
                                ));
                            }
                        }
                    }
                    ObjectManagerPhase::InitialRead => {
                        match read_managed_objects(state.proxy()).await {
                            Ok(objects) => {
                                state.objects = objects;
                                state.phase = ObjectManagerPhase::Watch;
                                return Some((Ok(sorted_objects(&state.objects)), state));
                            }
                            Err(error) => {
                                state.phase = ObjectManagerPhase::Done;
                                return Some((Err(error), state));
                            }
                        }
                    }
                    ObjectManagerPhase::Watch => {
                        if state.stream.is_none() {
                            match state.proxy().receive_all_signals().await {
                                Ok(stream) => state.stream = Some(Box::pin(stream)),
                                Err(error) => {
                                    state.phase = ObjectManagerPhase::Done;
                                    return Some((
                                        Err(format_dbus_error(
                                            "subscribe ObjectManager signals",
                                            error,
                                        )),
                                        state,
                                    ));
                                }
                            }
                        }

                        let Some(message) = state
                            .stream
                            .as_mut()
                            .expect("stream initialized")
                            .next()
                            .await
                        else {
                            return None;
                        };

                        match apply_object_manager_signal(&mut state.objects, message) {
                            ObjectManagerChange::Changed => {
                                return Some((Ok(sorted_objects(&state.objects)), state));
                            }
                            ObjectManagerChange::Ignore => continue,
                            ObjectManagerChange::Error(error) => {
                                state.phase = ObjectManagerPhase::Done;
                                return Some((Err(error), state));
                            }
                        }
                    }
                    ObjectManagerPhase::Done => return None,
                }
            }
        },
    )
}

impl ObjectManagerStreamState {
    fn proxy(&self) -> &Proxy<'static> {
        self.proxy.as_ref().expect("proxy initialized")
    }
}

fn object_model_stream<T>(
    descriptor: ObjectManagerDescriptor,
    interface: OwnedInterfaceName,
) -> impl Stream<Item = Result<Vec<T>, String>>
where
    T: ObjectModel,
{
    stream::unfold(
        ObjectModelStreamState {
            descriptor,
            interface,
            proxy: None,
            stream: None,
            paths: Vec::new(),
            phase: ObjectManagerPhase::Connect,
        },
        |mut state| async move {
            loop {
                match state.phase {
                    ObjectManagerPhase::Connect => {
                        match object_manager_proxy(&state.descriptor).await {
                            Ok(proxy) => {
                                state.proxy = Some(proxy);
                                state.phase = ObjectManagerPhase::InitialRead;
                            }
                            Err(error) => {
                                state.phase = ObjectManagerPhase::Done;
                                return Some((
                                    Err(format_dbus_error("connect ObjectManager source", error)),
                                    state,
                                ));
                            }
                        }
                    }
                    ObjectManagerPhase::InitialRead => {
                        match read_model_paths(state.proxy(), &state.interface).await {
                            Ok(paths) => {
                                state.paths = paths;
                                state.phase = ObjectManagerPhase::Watch;
                                return Some((Ok(model_list_from_paths::<T>(&state.paths)), state));
                            }
                            Err(error) => {
                                state.phase = ObjectManagerPhase::Done;
                                return Some((Err(error), state));
                            }
                        }
                    }
                    ObjectManagerPhase::Watch => {
                        if state.stream.is_none() {
                            match state.proxy().receive_all_signals().await {
                                Ok(stream) => state.stream = Some(Box::pin(stream)),
                                Err(error) => {
                                    state.phase = ObjectManagerPhase::Done;
                                    return Some((
                                        Err(format_dbus_error(
                                            "subscribe ObjectManager signals",
                                            error,
                                        )),
                                        state,
                                    ));
                                }
                            }
                        }

                        let Some(message) = state
                            .stream
                            .as_mut()
                            .expect("stream initialized")
                            .next()
                            .await
                        else {
                            return None;
                        };

                        match apply_object_model_signal(&mut state.paths, &state.interface, message)
                        {
                            ObjectManagerChange::Changed => {
                                return Some((Ok(model_list_from_paths::<T>(&state.paths)), state));
                            }
                            ObjectManagerChange::Ignore => continue,
                            ObjectManagerChange::Error(error) => {
                                state.phase = ObjectManagerPhase::Done;
                                return Some((Err(error), state));
                            }
                        }
                    }
                    ObjectManagerPhase::Done => return None,
                }
            }
        },
    )
}

impl ObjectModelStreamState {
    fn proxy(&self) -> &Proxy<'static> {
        self.proxy.as_ref().expect("proxy initialized")
    }
}

enum ObjectManagerChange {
    Changed,
    Ignore,
    Error(String),
}

async fn read_managed_objects(proxy: &Proxy<'static>) -> Result<ManagedObjects, String> {
    let objects = proxy
        .call::<_, _, RawManagedObjects>("GetManagedObjects", &())
        .await
        .map_err(|error| format_dbus_error("read managed objects", error))?;

    Ok(objects
        .into_iter()
        .map(|(path, interfaces)| {
            (
                path,
                interfaces
                    .into_iter()
                    .map(|(interface, properties)| (interface, arc_properties(properties)))
                    .collect(),
            )
        })
        .collect())
}

async fn read_model_paths(
    proxy: &Proxy<'static>,
    interface: &OwnedInterfaceName,
) -> Result<Vec<OwnedObjectPath>, String> {
    let objects = proxy
        .call::<_, _, RawManagedObjects>("GetManagedObjects", &())
        .await
        .map_err(|error| format_dbus_error("read managed objects", error))?;

    Ok(model_paths_from_managed_objects(objects, interface))
}

fn apply_object_manager_signal(
    objects: &mut ManagedObjects,
    message: Message,
) -> ObjectManagerChange {
    let Some(member) = message.header().member().map(|member| member.to_string()) else {
        return ObjectManagerChange::Ignore;
    };

    match member.as_str() {
        "InterfacesAdded" => {
            let body = message.body().deserialize::<(
                OwnedObjectPath,
                HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>,
            )>();
            match body {
                Ok((path, interfaces)) => {
                    objects.entry(path).or_default().extend(
                        interfaces
                            .into_iter()
                            .map(|(interface, properties)| (interface, arc_properties(properties))),
                    );
                    ObjectManagerChange::Changed
                }
                Err(error) => {
                    ObjectManagerChange::Error(format_dbus_error("decode InterfacesAdded", error))
                }
            }
        }
        "InterfacesRemoved" => {
            let body = message
                .body()
                .deserialize::<(OwnedObjectPath, Vec<OwnedInterfaceName>)>();
            match body {
                Ok((path, interfaces)) => {
                    if let Some(object) = objects.get_mut(&path) {
                        for interface in interfaces {
                            object.remove(&interface);
                        }
                        if object.is_empty() {
                            objects.remove(&path);
                        }
                    }
                    ObjectManagerChange::Changed
                }
                Err(error) => {
                    ObjectManagerChange::Error(format_dbus_error("decode InterfacesRemoved", error))
                }
            }
        }
        _ => ObjectManagerChange::Ignore,
    }
}

fn apply_object_model_signal(
    paths: &mut Vec<OwnedObjectPath>,
    interface: &OwnedInterfaceName,
    message: Message,
) -> ObjectManagerChange {
    let Some(member) = message.header().member().map(|member| member.to_string()) else {
        return ObjectManagerChange::Ignore;
    };

    match member.as_str() {
        "InterfacesAdded" => {
            let body = message.body().deserialize::<(
                OwnedObjectPath,
                HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>,
            )>();
            match body {
                Ok((path, interfaces)) => {
                    if interfaces.contains_key(interface) {
                        insert_model_path(paths, path)
                    } else {
                        ObjectManagerChange::Ignore
                    }
                }
                Err(error) => {
                    ObjectManagerChange::Error(format_dbus_error("decode InterfacesAdded", error))
                }
            }
        }
        "InterfacesRemoved" => {
            let body = message
                .body()
                .deserialize::<(OwnedObjectPath, Vec<OwnedInterfaceName>)>();
            match body {
                Ok((path, interfaces)) => {
                    if interfaces.iter().any(|removed| removed == interface) {
                        remove_model_path(paths, &path)
                    } else {
                        ObjectManagerChange::Ignore
                    }
                }
                Err(error) => {
                    ObjectManagerChange::Error(format_dbus_error("decode InterfacesRemoved", error))
                }
            }
        }
        _ => ObjectManagerChange::Ignore,
    }
}

fn model_paths_from_managed_objects(
    objects: RawManagedObjects,
    interface: &OwnedInterfaceName,
) -> Vec<OwnedObjectPath> {
    let mut paths = objects
        .into_iter()
        .filter_map(|(path, interfaces)| interfaces.contains_key(interface).then_some(path))
        .collect::<Vec<_>>();
    sort_model_paths(&mut paths);
    paths
}

fn model_list_from_paths<T>(paths: &[OwnedObjectPath]) -> Vec<T>
where
    T: ObjectModel,
{
    paths.iter().cloned().map(T::at).collect()
}

fn insert_model_path(
    paths: &mut Vec<OwnedObjectPath>,
    path: OwnedObjectPath,
) -> ObjectManagerChange {
    if paths.iter().any(|existing| existing == &path) {
        return ObjectManagerChange::Ignore;
    }

    paths.push(path);
    sort_model_paths(paths);
    ObjectManagerChange::Changed
}

fn remove_model_path(
    paths: &mut Vec<OwnedObjectPath>,
    path: &OwnedObjectPath,
) -> ObjectManagerChange {
    let Some(index) = paths.iter().position(|existing| existing == path) else {
        return ObjectManagerChange::Ignore;
    };

    paths.remove(index);
    ObjectManagerChange::Changed
}

fn sort_model_paths(paths: &mut [OwnedObjectPath]) {
    paths.sort_by(|left, right| left.as_str().cmp(right.as_str()));
}

fn sorted_objects(objects: &ManagedObjects) -> Vec<DbusObject> {
    let mut objects = objects
        .iter()
        .map(|(path, interfaces)| DbusObject {
            path: path.clone(),
            interfaces: sorted_interfaces(interfaces),
        })
        .collect::<Vec<_>>();
    objects.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
    objects
}

fn sorted_interfaces(
    interfaces: &HashMap<OwnedInterfaceName, HashMap<String, Arc<OwnedValue>>>,
) -> Vec<DbusInterface> {
    let mut interfaces = interfaces
        .iter()
        .map(|(name, properties)| DbusInterface {
            name: name.clone(),
            properties: sorted_properties(properties),
        })
        .collect::<Vec<_>>();
    interfaces.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
    interfaces
}

fn sorted_properties(properties: &HashMap<String, Arc<OwnedValue>>) -> Vec<DbusPropertyValue> {
    let mut properties = properties
        .iter()
        .map(|(name, value)| DbusPropertyValue {
            name: name.clone(),
            value: value.clone(),
        })
        .collect::<Vec<_>>();
    properties.sort_by(|left, right| left.name.cmp(&right.name));
    properties
}

fn arc_properties(properties: HashMap<String, OwnedValue>) -> HashMap<String, Arc<OwnedValue>> {
    properties
        .into_iter()
        .map(|(name, value)| (name, Arc::new(value)))
        .collect()
}

async fn properties_proxy(descriptor: &ObjectDescriptor) -> zbus::Result<Proxy<'static>> {
    interface_proxy(descriptor, DBUS_PROPERTIES).await
}

async fn object_proxy(descriptor: &ObjectDescriptor) -> zbus::Result<Proxy<'static>> {
    interface_proxy(descriptor, descriptor.interface.clone()).await
}

async fn object_manager_proxy(
    descriptor: &ObjectManagerDescriptor,
) -> zbus::Result<Proxy<'static>> {
    let connection = descriptor.bus.connection().await?;
    Proxy::new_owned(
        connection,
        descriptor.destination.clone(),
        descriptor.path.clone(),
        DBUS_OBJECT_MANAGER,
    )
    .await
}

async fn interface_proxy<I>(
    descriptor: &ObjectDescriptor,
    interface: I,
) -> zbus::Result<Proxy<'static>>
where
    I: TryInto<zbus::names::InterfaceName<'static>>,
    I::Error: Into<zbus::Error>,
{
    let connection = descriptor.bus.connection().await?;
    Proxy::new_owned(
        connection,
        descriptor.destination.clone(),
        descriptor.path.clone(),
        interface,
    )
    .await
}

fn parse_bus_name(value: &str) -> Result<OwnedBusName, String> {
    OwnedBusName::try_from(value).map_err(|error| format!("invalid bus name {value:?}: {error}"))
}

fn parse_interface_name(value: &str) -> Result<OwnedInterfaceName, String> {
    OwnedInterfaceName::try_from(value)
        .map_err(|error| format!("invalid interface name {value:?}: {error}"))
}

fn parse_object_path(value: &str) -> Result<OwnedObjectPath, String> {
    OwnedObjectPath::try_from(value)
        .map_err(|error| format!("invalid object path {value:?}: {error}"))
}

fn is_missing_property_error(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, Some(description), _)
            if name.as_str() == "org.freedesktop.DBus.Error.InvalidArgs" =>
        {
            is_missing_property_invalid_args(description)
        }
        zbus::Error::MethodError(name, _, _) => matches!(
            name.as_str(),
            "org.freedesktop.DBus.Error.UnknownProperty"
                | "org.freedesktop.DBus.Error.UnknownObject"
                | "org.freedesktop.DBus.Error.UnknownInterface"
        ),
        zbus::Error::FDO(error) => {
            matches!(
                error.as_ref(),
                zbus::fdo::Error::UnknownProperty(_)
                    | zbus::fdo::Error::UnknownObject(_)
                    | zbus::fdo::Error::UnknownInterface(_)
            ) || matches!(
                error.as_ref(),
                zbus::fdo::Error::InvalidArgs(message) if is_missing_property_invalid_args(message)
            )
        }
        _ => false,
    }
}

fn is_missing_property_invalid_args(message: &str) -> bool {
    message.contains("No such interface") || message.contains("No such property")
}

fn format_dbus_error(operation: &str, error: impl fmt::Display) -> String {
    format!("{operation} failed: {error}")
}

fn descriptor_error_path(key: &str) -> PathBuf {
    PathBuf::from(format!("dbus/{key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_descriptor_rejects_invalid_parts() {
        assert!(
            ObjectDescriptor::parse(
                Bus::Session,
                "io.github.Test",
                "/io/github/Test",
                "io.github.Test1"
            )
            .is_ok()
        );
        assert!(
            ObjectDescriptor::parse(
                Bus::Session,
                "not a bus",
                "/io/github/Test",
                "io.github.Test1"
            )
            .is_err()
        );
        assert!(
            ObjectDescriptor::parse(
                Bus::Session,
                "io.github.Test",
                "not/absolute",
                "io.github.Test1"
            )
            .is_err()
        );
        assert!(
            ObjectDescriptor::parse(
                Bus::Session,
                "io.github.Test",
                "/io/github/Test",
                "not an interface"
            )
            .is_err()
        );
    }

    #[test]
    fn descriptor_keys_include_bus_object_interface_and_member() {
        let object = ObjectDescriptor::parse(
            Bus::System,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )
        .expect("valid descriptor");

        let property = PropertyDescriptor::new(object.clone(), "OnBattery");
        let signal = SignalDescriptor::new(object, "DeviceAdded");

        assert_eq!(
            property.key(),
            "system:org.freedesktop.UPower:/org/freedesktop/UPower:org.freedesktop.UPower:OnBattery"
        );
        assert_eq!(
            signal.key(),
            "system:org.freedesktop.UPower:/org/freedesktop/UPower:org.freedesktop.UPower:DeviceAdded"
        );
    }

    #[test]
    fn object_manager_descriptor_key_is_stable() {
        let descriptor =
            ObjectManagerDescriptor::parse(Bus::Session, "org.rsynapse.Niri", "/org/rsynapse/Niri")
                .expect("valid descriptor");

        assert_eq!(
            descriptor.key(),
            "session:org.rsynapse.Niri:/org/rsynapse/Niri"
        );
    }

    #[test]
    fn missing_property_errors_include_no_such_interface_invalid_args() {
        let error = zbus::Error::FDO(Box::new(zbus::fdo::Error::InvalidArgs(
            "No such interface 'org.bluez.Battery1'".to_string(),
        )));

        assert!(is_missing_property_error(&error));
    }

    #[test]
    fn missing_property_errors_include_no_such_property_invalid_args() {
        let error = zbus::Error::FDO(Box::new(zbus::fdo::Error::InvalidArgs(
            "No such property 'Connecting'".to_string(),
        )));

        assert!(is_missing_property_error(&error));
    }

    #[test]
    fn missing_property_errors_do_not_hide_other_invalid_args() {
        let error = zbus::Error::FDO(Box::new(zbus::fdo::Error::InvalidArgs(
            "Bad property name".to_string(),
        )));

        assert!(!is_missing_property_error(&error));
    }

    #[test]
    fn model_paths_filter_and_sort_requested_interface() {
        let interface = parse_interface_name("org.example.Target").unwrap();
        let other = parse_interface_name("org.example.Other").unwrap();
        let first = parse_object_path("/org/example/first").unwrap();
        let second = parse_object_path("/org/example/second").unwrap();
        let mut objects = RawManagedObjects::new();
        objects.insert(
            second.clone(),
            HashMap::from([(interface.clone(), HashMap::new())]),
        );
        objects.insert(first, HashMap::from([(other, HashMap::new())]));

        assert_eq!(
            model_paths_from_managed_objects(objects, &interface),
            vec![second]
        );
    }

    #[test]
    fn model_path_updates_report_real_membership_changes_only() {
        let one = parse_object_path("/org/example/one").unwrap();
        let two = parse_object_path("/org/example/two").unwrap();
        let mut paths = vec![one.clone()];

        assert!(matches!(
            insert_model_path(&mut paths, one.clone()),
            ObjectManagerChange::Ignore
        ));
        assert!(matches!(
            insert_model_path(&mut paths, two.clone()),
            ObjectManagerChange::Changed
        ));
        assert_eq!(paths, vec![one, two.clone()]);
        assert!(matches!(
            remove_model_path(&mut paths, &two),
            ObjectManagerChange::Changed
        ));
        assert!(matches!(
            remove_model_path(&mut paths, &two),
            ObjectManagerChange::Ignore
        ));
    }
}
