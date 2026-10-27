use std::collections::BTreeMap;

use crate::Value;

use super::adapter::BluetoothAdapter;
use super::device::BluetoothDevice;
use super::{ADAPTER, DEVICE};

/*
 * what bluez has, as GetManagedObjects lists it: object path -> interface
 * name -> that interface's properties (a Value::Map); kept up to date from
 * bluez's own signals instead of being asked for again
 */
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Objects {
    objects: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Objects {
    pub(crate) const fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
        }
    }

    // the answer to GetManagedObjects, an empty tree when bluez isn't running
    pub(crate) fn from_managed(managed: &Value) -> Self {
        let mut objects = Self::default();

        let Value::Map(paths) = managed else {
            return objects;
        };

        for (path, interfaces) in paths {
            objects.add(path, interfaces);
        }

        objects
    }

    // InterfacesAdded: the object and the interfaces it gained, with all their properties
    pub(crate) fn added(&mut self, arguments: &[Value]) {
        let [path, interfaces, ..] = arguments else {
            return;
        };

        self.add(path.text(), interfaces);
    }

    // InterfacesRemoved: the object and the names of the interfaces it lost
    pub(crate) fn removed(&mut self, arguments: &[Value]) {
        let [path, interfaces, ..] = arguments else {
            return;
        };

        let Some(object) = self.objects.get_mut(path.text()) else {
            return;
        };

        for interface in interfaces.list() {
            object.remove(interface.text());
        }

        // an object without interfaces is gone
        if object.is_empty() {
            self.objects.remove(path.text());
        }
    }

    /*
     * PropertiesChanged on `path`: the interface, the new values, and the
     * names of properties that changed without saying to what, which are
     * dropped so a stale value never shows
     */
    pub(crate) fn changed(&mut self, path: &str, arguments: &[Value]) {
        let [interface, values, rest @ ..] = arguments else {
            return;
        };

        // a change for an object not seen yet waits for its InterfacesAdded
        let Some(object) = self.objects.get_mut(path) else {
            return;
        };

        let properties = object
            .entry(interface.text().to_string())
            .or_insert_with(|| Value::Map(BTreeMap::new()));

        let Value::Map(properties) = properties else {
            return;
        };

        if let Value::Map(values) = values {
            for (name, value) in values {
                properties.insert(name.clone(), value.clone());
            }
        }

        for name in rest.first().map(Value::list).unwrap_or_default() {
            properties.remove(name.text());
        }
    }

    fn add(&mut self, path: &str, interfaces: &Value) {
        let Value::Map(interfaces) = interfaces else {
            return;
        };

        let object = self.objects.entry(path.to_string()).or_default();

        for (interface, properties) in interfaces {
            object.insert(interface.clone(), properties.clone());
        }
    }

    // the interfaces of one object, as from_interfaces() reads them
    fn interfaces(&self, path: &str) -> Value {
        let Some(object) = self.objects.get(path) else {
            return Value::Nothing;
        };

        Value::Map(object.clone().into_iter().collect())
    }

    /*
     * the adapter shown and its devices: the first powered adapter by path,
     * else the first one, so the choice never flips between two
     */
    pub(crate) fn view(&self) -> (Option<BluetoothAdapter>, Vec<BluetoothDevice>) {
        let adapters: Vec<BluetoothAdapter> = self
            .objects
            .iter()
            .filter_map(|(path, object)| {
                let adapter = object.get(ADAPTER)?;

                Some(BluetoothAdapter::from_properties(path, adapter))
            })
            .collect();

        let adapter = adapters
            .iter()
            .find(|adapter| adapter.powered())
            .or(adapters.first())
            .cloned();

        let Some(adapter) = adapter else {
            return (None, Vec::new());
        };

        let mut devices: Vec<BluetoothDevice> = self
            .objects
            .iter()
            .filter(|(_, object)| {
                let device = object.get(DEVICE).unwrap_or(&Value::Nothing);

                device != &Value::Nothing && device.get("Adapter").text() == adapter.path()
            })
            .map(|(path, _)| BluetoothDevice::from_interfaces(path, &self.interfaces(path)))
            .collect();

        // connected first, then paired, then by name
        devices.sort_by_key(|device| {
            let order = (!device.connected, !device.paired);

            (order, device.name.to_lowercase(), device.path.clone())
        });

        (Some(adapter), devices)
    }

    #[cfg(test)]
    pub(crate) fn device(&self, path: &str) -> Option<BluetoothDevice> {
        let object = self.objects.get(path)?;

        object.get(DEVICE)?;

        Some(BluetoothDevice::from_interfaces(
            path,
            &self.interfaces(path),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
        )
    }

    fn text(text: &str) -> Value {
        Value::Text(text.to_string())
    }

    fn adapter(powered: bool) -> Value {
        map(&[(
            ADAPTER,
            map(&[("Powered", Value::Bool(powered)), ("Alias", text("nixos"))]),
        )])
    }

    fn device(adapter: &str, name: &str, connected: bool) -> Value {
        map(&[(
            DEVICE,
            map(&[
                ("Adapter", text(adapter)),
                ("Alias", text(name)),
                ("Address", text("00:11:22:33:44:55")),
                ("Paired", Value::Bool(true)),
                ("Connected", Value::Bool(connected)),
            ]),
        )])
    }

    fn tree() -> Objects {
        Objects::from_managed(&map(&[
            ("/org/bluez/hci0", adapter(true)),
            (
                "/org/bluez/hci0/dev_a",
                device("/org/bluez/hci0", "Zed", false),
            ),
            (
                "/org/bluez/hci0/dev_b",
                device("/org/bluez/hci0", "Alpha", false),
            ),
        ]))
    }

    #[test]
    fn reads_the_adapter_and_its_devices() {
        let (adapter, devices) = tree().view();

        assert_eq!(adapter.unwrap().path(), "/org/bluez/hci0");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name(), "Alpha");
    }

    #[test]
    fn a_property_change_lands_on_its_device() {
        let mut objects = tree();

        objects.changed(
            "/org/bluez/hci0/dev_a",
            &[
                text(DEVICE),
                map(&[("Connected", Value::Bool(true))]),
                Value::List(vec![]),
            ],
        );

        let (_, devices) = objects.view();

        // connected now sorts first
        assert_eq!(devices[0].name(), "Zed");
        assert!(devices[0].connected());
    }

    #[test]
    fn an_invalidated_property_is_dropped() {
        let mut objects = tree();

        objects.changed(
            "/org/bluez/hci0/dev_a",
            &[text(DEVICE), map(&[]), Value::List(vec![text("Alias")])],
        );

        let device = objects.device("/org/bluez/hci0/dev_a").unwrap();

        // an unnamed device shows its address
        assert_eq!(device.name(), "00:11:22:33:44:55");
    }

    #[test]
    fn a_change_for_an_unknown_object_is_ignored() {
        let mut objects = tree();
        let before = objects.clone();

        objects.changed(
            "/org/bluez/hci0/dev_gone",
            &[text(DEVICE), map(&[("Connected", Value::Bool(true))])],
        );

        assert_eq!(objects, before);
    }

    #[test]
    fn devices_come_and_go() {
        let mut objects = tree();

        objects.added(&[
            text("/org/bluez/hci0/dev_c"),
            device("/org/bluez/hci0", "Mid", false),
        ]);
        assert_eq!(objects.view().1.len(), 3);

        objects.removed(&[
            text("/org/bluez/hci0/dev_c"),
            Value::List(vec![text(DEVICE)]),
        ]);
        assert_eq!(objects.view().1.len(), 2);
        assert!(objects.device("/org/bluez/hci0/dev_c").is_none());
    }

    #[test]
    fn losing_one_interface_keeps_the_others() {
        let mut objects = tree();

        objects.added(&[
            text("/org/bluez/hci0/dev_a"),
            map(&[(
                "org.bluez.Battery1",
                map(&[("Percentage", Value::Number(80.0))]),
            )]),
        ]);
        assert_eq!(
            objects.device("/org/bluez/hci0/dev_a").unwrap().battery(),
            Some(80)
        );

        objects.removed(&[
            text("/org/bluez/hci0/dev_a"),
            Value::List(vec![text("org.bluez.Battery1")]),
        ]);

        let device = objects.device("/org/bluez/hci0/dev_a").unwrap();
        assert_eq!(device.battery(), None);
        assert_eq!(device.name(), "Zed");
    }

    #[test]
    fn a_powered_adapter_wins_and_owns_its_devices() {
        let objects = Objects::from_managed(&map(&[
            ("/org/bluez/hci0", adapter(false)),
            ("/org/bluez/hci1", adapter(true)),
            (
                "/org/bluez/hci0/dev_a",
                device("/org/bluez/hci0", "Old", false),
            ),
            (
                "/org/bluez/hci1/dev_b",
                device("/org/bluez/hci1", "New", false),
            ),
        ]));

        let (adapter, devices) = objects.view();

        assert_eq!(adapter.unwrap().path(), "/org/bluez/hci1");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name(), "New");
    }

    #[test]
    fn no_bluez_is_no_adapter() {
        let (adapter, devices) = Objects::from_managed(&Value::Nothing).view();

        assert!(adapter.is_none());
        assert!(devices.is_empty());
    }
}
