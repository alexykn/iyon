use std::{any::Any, collections::HashMap, num::NonZeroU64};

use super::{Component, ComponentHandle, ComponentId};
use crate::presentation::View;

trait ErasedComponent: std::fmt::Debug {
    fn view(&self) -> View;
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

/// The sole owner of retained component instances.
#[derive(Debug)]
pub(crate) struct ComponentRegistry {
    slots: HashMap<ComponentId, Box<dyn ErasedComponent>>,
    next_id: Option<NonZeroU64>,
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
            next_id: NonZeroU64::new(1),
        }
    }

    pub(crate) fn register<C>(&mut self, component: C) -> ComponentHandle<C>
    where
        C: Component,
    {
        let value = self.next_id.take().expect("component id exhausted");
        // The maximum nonzero ID was just allocated. The next registration
        // must fail explicitly rather than reusing or aliasing it.
        self.next_id = value.get().checked_add(1).and_then(NonZeroU64::new);
        let id = ComponentId::from_nonzero(value);
        self.slots.insert(id, Box::new(component));
        ComponentHandle::new(id)
    }

    pub(crate) fn contains<C>(&self, handle: ComponentHandle<C>) -> bool
    where
        C: Component,
    {
        self.slots
            .get(&handle.id())
            .is_some_and(|component| component.as_any().is::<C>())
    }

    pub(crate) fn with<C, R>(
        &self,
        handle: ComponentHandle<C>,
        f: impl FnOnce(&C) -> R,
    ) -> Option<R>
    where
        C: Component,
    {
        let component = self.slots.get(&handle.id())?;
        let component = component.as_any().downcast_ref::<C>()?;
        Some(f(component))
    }

    pub(crate) fn with_mut<C, R>(
        &mut self,
        handle: ComponentHandle<C>,
        f: impl FnOnce(&mut C) -> R,
    ) -> Option<R>
    where
        C: Component,
    {
        let component = self.slots.get_mut(&handle.id())?;
        let component = component.as_any_mut().downcast_mut::<C>()?;
        Some(f(component))
    }

    pub(crate) fn render<C>(&self, handle: ComponentHandle<C>) -> Option<View>
    where
        C: Component,
    {
        let component = self.slots.get(&handle.id())?;
        if !component.as_any().is::<C>() {
            return None;
        }
        Some(component.view().attach_component(handle.id()))
    }

    pub(crate) fn remove<C>(&mut self, handle: ComponentHandle<C>) -> Option<C>
    where
        C: Component,
    {
        let component = self.slots.get(&handle.id())?;
        if !component.as_any().is::<C>() {
            return None;
        }
        self.slots
            .remove(&handle.id())?
            .into_any()
            .downcast::<C>()
            .ok()
            .map(|component| *component)
    }

    /// Internal host operation for resolving a registered identity without
    /// exposing erased objects or raw mutable state.
    pub(crate) fn render_registered(&self, id: ComponentId) -> Option<View> {
        let component = self.slots.get(&id)?;
        Some(component.view().attach_component(id))
    }
}
