use std::{any::Any, collections::HashMap, fmt};

use super::{Component, ComponentHandle, ComponentId, ComponentRevision};
use crate::interaction::{ComponentCapabilities, ComponentCx};
use crate::presentation::View;

trait ErasedComponent {
    fn view(&self) -> View;
    fn capabilities(&self) -> ComponentCapabilities;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<C> ErasedComponent for C
where
    C: Component,
{
    fn view(&self) -> View {
        Component::view(self)
    }

    fn capabilities(&self) -> ComponentCapabilities {
        let mut capabilities = ComponentCapabilities::default();
        let mut cx = ComponentCx::new(&mut capabilities);
        Component::capabilities(self, &mut cx);
        capabilities
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

struct ComponentEntry {
    component: Box<dyn ErasedComponent>,
    revision: ComponentRevision,
}

impl fmt::Debug for ComponentEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ComponentEntry(..)")
    }
}

/// The sole owner of retained component instances.
#[derive(Debug)]
pub(crate) struct ComponentRegistry {
    slots: HashMap<ComponentId, ComponentEntry>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentRegistry {
    pub(crate) fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    pub(crate) fn register<C>(&mut self, component: C) -> ComponentHandle<C>
    where
        C: Component,
    {
        let id = ComponentId::allocate();
        self.slots.insert(
            id,
            ComponentEntry {
                component: Box::new(component),
                revision: ComponentRevision::default(),
            },
        );
        ComponentHandle::new(id)
    }

    pub(crate) fn contains<C>(&self, handle: ComponentHandle<C>) -> bool
    where
        C: Component,
    {
        self.slots
            .get(&handle.id())
            .is_some_and(|entry| entry.component.as_any().is::<C>())
    }

    pub(crate) fn with<C, R>(
        &self,
        handle: ComponentHandle<C>,
        f: impl FnOnce(&C) -> R,
    ) -> Option<R>
    where
        C: Component,
    {
        let entry = self.slots.get(&handle.id())?;
        let component = entry.component.as_any().downcast_ref::<C>()?;
        Some(f(component))
    }

    pub(crate) fn with_any<R>(&self, id: ComponentId, f: impl FnOnce(&dyn Any) -> R) -> Option<R> {
        let entry = self.slots.get(&id)?;
        Some(f(entry.component.as_any()))
    }

    pub(crate) fn with_any_mut<R>(
        &mut self,
        id: ComponentId,
        f: impl FnOnce(&mut dyn Any) -> R,
    ) -> Option<R> {
        let entry = self.slots.get_mut(&id)?;
        let result = f(entry.component.as_any_mut());
        entry.revision = entry.revision.increment();
        Some(result)
    }

    pub(crate) fn capabilities(&self, id: ComponentId) -> Option<ComponentCapabilities> {
        self.slots
            .get(&id)
            .map(|entry| entry.component.capabilities())
    }

    pub(crate) fn with_mut<C, R>(
        &mut self,
        handle: ComponentHandle<C>,
        f: impl FnOnce(&mut C) -> R,
    ) -> Option<R>
    where
        C: Component,
    {
        let entry = self.slots.get_mut(&handle.id())?;
        let component = entry.component.as_any_mut().downcast_mut::<C>()?;
        let result = f(component);
        entry.revision = entry.revision.increment();
        Some(result)
    }

    pub(crate) fn render<C>(&self, handle: ComponentHandle<C>) -> Option<View>
    where
        C: Component,
    {
        let entry = self.slots.get(&handle.id())?;
        if !entry.component.as_any().is::<C>() {
            return None;
        }
        Some(entry.component.view().attach_component(handle.id()))
    }

    pub(crate) fn remove<C>(&mut self, handle: ComponentHandle<C>) -> Option<C>
    where
        C: Component,
    {
        let entry = self.slots.get(&handle.id())?;
        if !entry.component.as_any().is::<C>() {
            return None;
        }
        self.slots
            .remove(&handle.id())?
            .component
            .into_any()
            .downcast::<C>()
            .ok()
            .map(|component| *component)
    }

    pub(crate) fn revision<C>(&self, handle: ComponentHandle<C>) -> Option<ComponentRevision>
    where
        C: Component,
    {
        let entry = self.slots.get(&handle.id())?;
        entry.component.as_any().is::<C>().then_some(entry.revision)
    }

    /// Resolver-only snapshot access. Unlike `render`, this returns the raw
    /// semantic component view without attaching ownership metadata.
    pub(crate) fn view_for_resolution(&self, id: ComponentId) -> Option<(View, ComponentRevision)> {
        let entry = self.slots.get(&id)?;
        Some((entry.component.view(), entry.revision))
    }

    /// Internal host operation for resolving a registered identity without
    /// exposing erased objects or raw mutable state.
    pub(crate) fn render_registered(&self, id: ComponentId) -> Option<View> {
        let (view, _) = self.view_for_resolution(id)?;
        Some(view.attach_component(id))
    }
}
