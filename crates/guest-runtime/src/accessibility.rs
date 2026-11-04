//! Read live AT-SPI controls from the observed foreground window, never a background app.
use crate::{Result, Settings, screen::Screen};
use atspi::{
    CoordType, Interface, ObjectRefOwned, RelationType, State, StateSet,
    proxy::{accessible::AccessibleProxy, component::ComponentProxy},
    zbus::{self, Connection, proxy::CacheProperties},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};

const ROOT: &str = "/org/a11y/atspi/accessible/root";
const MAX_NODES: usize = 384;

#[derive(Deserialize)]
struct Query {
    window_id: u32,
    process_id: u32,
    #[serde(default)]
    text: String,
    #[serde(default)]
    role: String,
}

fn same_window(expected: &Query, current: &crate::screen::WindowIdentity) -> Result<()> {
    if expected.window_id == 0
        || expected.process_id == 0
        || expected.window_id != current.window_id
        || expected.process_id != current.process_id
    {
        return Err("前台窗口已变化或旧观察没有窗口标识，请重新computer_observe".into());
    }
    Ok(())
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace() && !"_~-".contains(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

impl Query {
    fn validate(&self) -> Result<()> {
        if self.text.chars().count() > 200
            || self.role.len() > 80
            || normalized(&self.text).is_empty() && normalized(&self.role).is_empty()
        {
            return Err("需要控件名称或角色，名称不超过200字".into());
        }
        Ok(())
    }
}

async fn connection(settings: &Settings) -> Result<Connection> {
    let env = settings.environment()?;
    let address = env
        .iter()
        .find(|(k, _)| k == "DBUS_SESSION_BUS_ADDRESS")
        .and_then(|(_, v)| v.to_str())
        .ok_or("桌面会话总线不可用")?;
    let session = zbus::connection::Builder::address(address)?.build().await?;
    let bus = zbus::Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus").await?;
    let address: String = bus.call("GetAddress", &()).await?;
    Ok(zbus::connection::Builder::address(address.as_str())?
        .build()
        .await?)
}

async fn accessible<'a>(
    bus: &'a Connection,
    object: &'a ObjectRefOwned,
) -> Result<AccessibleProxy<'a>> {
    Ok(AccessibleProxy::builder(bus)
        .destination(object.name_as_str().ok_or("空控件引用")?)?
        .path(object.path_as_str())?
        .cache_properties(CacheProperties::No)
        .build()
        .await?)
}

async fn roots(bus: &Connection) -> Result<Vec<ObjectRefOwned>> {
    let registry = AccessibleProxy::builder(bus)
        .destination("org.a11y.atspi.Registry")?
        .path(ROOT)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    Ok(registry.get_children().await?)
}
async fn owner_pid(dbus: &zbus::fdo::DBusProxy<'_>, app: &ObjectRefOwned) -> Result<u32> {
    let name = app.name_as_str().ok_or("空应用引用")?;
    Ok(dbus
        .get_connection_unix_process_id(zbus::names::BusName::try_from(name)?)
        .await?)
}
async fn active_children(bus: &Connection, app: &ObjectRefOwned) -> Result<Vec<ObjectRefOwned>> {
    let root = accessible(bus, app).await?;
    let mut active = vec![];
    for window in root.get_children().await? {
        if accessible(bus, &window)
            .await?
            .get_state()
            .await?
            .contains(State::Active)
        {
            active.push(window);
        }
    }
    Ok(active)
}
async fn active_window(bus: &Connection, pid: u32) -> Result<ObjectRefOwned> {
    let dbus = zbus::fdo::DBusProxy::new(bus).await?;
    let mut windows = vec![];
    for app in roots(bus).await? {
        if owner_pid(&dbus, &app).await? == pid {
            windows.extend(active_children(bus, &app).await?);
        }
    }
    if windows.len() != 1 {
        return Err("当前应用未提供唯一的活动可访问窗口，可使用computer_find_text".into());
    }
    Ok(windows.remove(0))
}

async fn label(bus: &Connection, object: &AccessibleProxy<'_>) -> Result<String> {
    let name = object.name().await?;
    if !name.is_empty() {
        return Ok(name);
    }
    let mut labels = vec![];
    for (relation, targets) in object.get_relation_set().await? {
        if relation != RelationType::LabelledBy {
            continue;
        }
        for target in targets.iter().take(8) {
            labels.push(accessible(bus, target).await?.name().await?);
        }
    }
    Ok(labels.join(" "))
}

#[derive(Serialize)]
struct Bounds {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
}
impl Bounds {
    fn center(&self) -> [i64; 2] {
        [(self.left + self.right) / 2, (self.top + self.bottom) / 2]
    }
}
fn bounds((x, y, width, height): (i32, i32, i32, i32), settings: &Settings) -> Option<Bounds> {
    let left = i64::from(x).max(0);
    let top = i64::from(y).max(0);
    let right = (i64::from(x) + i64::from(width)).min(i64::from(settings.width));
    let bottom = (i64::from(y) + i64::from(height)).min(i64::from(settings.height));
    (width > 0 && height > 0 && right > left && bottom > top).then_some(Bounds {
        left,
        top,
        right,
        bottom,
    })
}

fn visible(state: StateSet) -> bool {
    state.contains(State::Showing)
        && state.contains(State::Visible)
        && !state.contains(State::Defunct)
}
async fn control_bounds(
    bus: &Connection,
    object: &ObjectRefOwned,
    settings: &Settings,
) -> Result<Option<Bounds>> {
    let component = ComponentProxy::builder(bus)
        .destination(object.name_as_str().ok_or("空控件引用")?)?
        .path(object.path_as_str())?
        .build()
        .await?;
    Ok(bounds(
        component.get_extents(CoordType::Screen).await?,
        settings,
    ))
}
async fn control_role(
    proxy: &AccessibleProxy<'_>,
    query: &Query,
) -> Result<Option<(String, atspi::InterfaceSet)>> {
    let (role, interfaces) = tokio::try_join!(proxy.get_role(), proxy.get_interfaces())?;
    let role = role.to_string();
    if !query.role.is_empty() && normalized(&query.role) != normalized(&role) {
        return Ok(None);
    }
    Ok(interfaces
        .contains(Interface::Component)
        .then_some((role, interfaces)))
}
async fn candidate(
    bus: &Connection,
    object: &ObjectRefOwned,
    proxy: &AccessibleProxy<'_>,
    state: StateSet,
    query: &Query,
    settings: &Settings,
) -> Result<Option<Value>> {
    let name = label(bus, proxy).await?;
    if !normalized(&name).contains(&normalized(&query.text)) {
        return Ok(None);
    }
    let Some((role, interfaces)) = control_role(proxy, query).await? else {
        return Ok(None);
    };
    let Some(bounds) = control_bounds(bus, object, settings).await? else {
        return Ok(None);
    };
    Ok(Some(
        json!({"name":name,"role":role,"bounds":bounds,"center":bounds.center(),
        "enabled":state.contains(State::Enabled) && state.contains(State::Sensitive),
        "editable":state.contains(State::Editable),"focused":state.contains(State::Focused),
        "actionable":interfaces.contains(Interface::Action) || state.contains(State::Focusable)}),
    ))
}

struct Traversal {
    pending: VecDeque<(ObjectRefOwned, u8)>,
    seen: HashSet<(String, String)>,
    partial: bool,
}
impl Traversal {
    fn new(window: ObjectRefOwned) -> Self {
        Self {
            pending: VecDeque::from([(window, 0)]),
            seen: HashSet::new(),
            partial: false,
        }
    }
    fn next(&mut self) -> Option<(ObjectRefOwned, u8)> {
        if self.seen.len() == MAX_NODES {
            self.partial |= !self.pending.is_empty();
            return None;
        }
        while let Some((object, depth)) = self.pending.pop_front() {
            let key = (
                object.name_as_str().unwrap_or("").to_owned(),
                object.path_as_str().to_owned(),
            );
            if self.seen.insert(key) {
                return Some((object, depth));
            }
        }
        None
    }
    fn remaining(&self) -> usize {
        MAX_NODES.saturating_sub(self.seen.len() + self.pending.len())
    }
    async fn descend(
        &mut self,
        proxy: &AccessibleProxy<'_>,
        state: StateSet,
        depth: u8,
    ) -> Result<()> {
        if state.contains(State::ManagesDescendants) || depth == 12 {
            self.partial = true;
            return Ok(());
        }
        let (children, truncated) = children(proxy, self.remaining()).await?;
        self.partial |= truncated;
        self.pending
            .extend(children.into_iter().map(|child| (child, depth + 1)));
        Ok(())
    }
}
async fn children(
    proxy: &AccessibleProxy<'_>,
    limit: usize,
) -> Result<(Vec<ObjectRefOwned>, bool)> {
    let count = proxy.child_count().await?.max(0) as usize;
    if count <= limit {
        return Ok((proxy.get_children().await?, false));
    }
    let mut children = vec![];
    for index in 0..limit {
        children.push(proxy.get_child_at_index(index as i32).await?);
    }
    Ok((children, true))
}
async fn search(
    bus: &Connection,
    window: ObjectRefOwned,
    query: &Query,
    settings: &Settings,
) -> Result<Value> {
    let mut traversal = Traversal::new(window);
    let mut candidates = vec![];
    while let Some((object, depth)) = traversal.next() {
        let proxy = accessible(bus, &object).await?;
        let state = proxy.get_state().await?;
        if !visible(state) {
            continue;
        }
        if let Some(found) = candidate(bus, &object, &proxy, state, query, settings).await? {
            candidates.push(found);
        }
        traversal.descend(&proxy, state, depth).await?;
    }
    Ok(
        json!({"source":"accessibility","candidates":candidates,"partial":traversal.partial,"inspected_nodes":traversal.seen.len()}),
    )
}
async fn inspect(query: &Query, settings: &Settings) -> Result<Value> {
    let bus = connection(settings).await?;
    let window = active_window(&bus, query.process_id).await?;
    search(&bus, window, query, settings).await
}

pub async fn find(screen: &impl Screen, settings: &Settings, args: Value) -> Result<Value> {
    let query: Query = serde_json::from_value(args)?;
    query.validate()?;
    same_window(&query, &screen.window()?)?;
    let result = inspect(&query, settings).await?;
    same_window(&query, &screen.window()?)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inconsistent_child_counts_cannot_exceed_the_inspection_budget() {
        let object = |index| {
            atspi::ObjectRef::new_owned(
                zbus::names::UniqueName::try_from(":1.2").unwrap(),
                zbus::zvariant::ObjectPath::try_from(format!("/org/a11y/test/{index}")).unwrap(),
            )
        };
        let mut traversal = Traversal::new(object(0));
        traversal
            .pending
            .extend((1..MAX_NODES + 8).map(|i| (object(i), 1)));
        let mut inspected = 0;
        while traversal.next().is_some() {
            inspected += 1;
        }
        assert_eq!(inspected, MAX_NODES);
        assert!(traversal.partial);
        let mut duplicate = Traversal::new(object(0));
        duplicate.pending.push_back((object(0), 1));
        assert!(duplicate.next().is_some());
        assert!(duplicate.next().is_none());
        assert!(!duplicate.partial);
    }

    #[test]
    fn inspection_rejects_other_window_or_process_and_legacy_identity() {
        let current = crate::screen::WindowIdentity {
            window_id: 7,
            process_id: 42,
            ..Default::default()
        };
        let mut query = Query {
            window_id: 7,
            process_id: 42,
            text: "OK".into(),
            role: String::new(),
        };
        assert!(same_window(&query, &current).is_ok());
        query.window_id = 8;
        assert!(same_window(&query, &current).is_err());
        query.window_id = 7;
        query.process_id = 43;
        assert!(same_window(&query, &current).is_err());
        query.process_id = 0;
        assert!(same_window(&query, &current).is_err());
        query.window_id = 0;
        query.process_id = 42;
        assert!(same_window(&query, &current).is_err());
        for (window_id, process_id) in [(0, 0), (0, 42), (7, 0)] {
            let current = crate::screen::WindowIdentity {
                window_id,
                process_id,
                ..Default::default()
            };
            query.window_id = window_id;
            query.process_id = process_id;
            assert!(
                same_window(&query, &current).is_err(),
                "matching zero identities never establish a foreground window"
            );
        }
    }

    #[test]
    fn screen_bounds_clip_visible_parts_without_integer_wrap_or_empty_targets() {
        let s = Settings::from_env();
        assert_eq!(
            serde_json::to_value(bounds((-10, 30, 40, 20), &s).unwrap()).unwrap(),
            json!({"left":0,"top":30,"right":30,"bottom":50})
        );
        assert!(bounds((1280, 20, 10, 10), &s).is_none());
        assert!(bounds((10, 20, 0, 10), &s).is_none());
        assert!(bounds((i32::MAX, 20, i32::MAX, 20), &s).is_none());
        assert!(bounds((10, 20, 10, -1), &s).is_none());
    }

    #[test]
    fn mnemonic_and_role_matching_preserve_non_latin_labels() {
        assert_eq!(normalized("_OK"), normalized("ok"));
        assert_eq!(normalized("Push_Button"), normalized("push button"));
        assert_eq!(normalized("保存"), "保存");
        let q = Query {
            window_id: 1,
            process_id: 2,
            text: "_ ~".into(),
            role: String::new(),
        };
        assert!(q.validate().is_err());
    }
}
