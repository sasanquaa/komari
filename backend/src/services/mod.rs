use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    fmt::{self, Debug},
    rc::Rc,
    sync::Arc,
};

use log::{debug, error};
use platforms::{Window, input::InputKind};
use tokio::{
    select, spawn,
    sync::{broadcast::Receiver, mpsc},
};

#[cfg(debug_assertions)]
use crate::services::debug::DebugService;
use crate::{
    Localization, Settings,
    bridge::{Capture, DefaultInputReceiver, Input, InputMethod},
    database_event_receiver,
    ecs::{Resources, World, WorldEvent},
    rotator::Rotator,
    services::{
        capture::{CaptureService, DefaultCaptureService},
        character::{CharacterService, DefaultCharacterService},
        database::DatabaseEventHandler,
        input::{DefaultInputService, InputEventHandler, InputService},
        localization::{DefaultLocalizationService, LocalizationService},
        map::{DefaultMapService, MapService},
        mediator::{DefaultMediatorService, MediatorEventHandler, MediatorService},
        operation::{DefaultOperationService, OperationEventHandler, OperationService},
        rotator::{DefaultRotatorService, RotatorService},
        settings::{DefaultSettingsService, SettingsService},
        world::WorldEventHandler,
    },
};

mod capture;
mod character;
mod database;
#[cfg(debug_assertions)]
mod debug;
mod input;
mod localization;
mod map;
mod mediator;
mod operation;
mod rotator;
mod settings;
mod world;

pub trait Event: Any + Send + Sync + Debug + 'static {}

trait EventHandler<E: Event> {
    fn handle(&mut self, context: &mut EventContext<'_>, event: E);
}

type EventHandlerFn = Box<dyn FnMut(&mut EventContext<'_>, Box<dyn Any>)>;

struct EventBus {
    handlers: HashMap<TypeId, EventHandlerFn>,
}

impl fmt::Debug for EventBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventBus")
            .field("handlers", &"HashMap { ... }")
            .finish()
    }
}

impl EventBus {
    fn subscribe<E: Event, H: EventHandler<E> + 'static>(&mut self, mut handler: H) {
        self.handlers
            .entry(TypeId::of::<E>())
            .or_insert(Box::new(move |context, event| {
                handler.handle(context, Box::into_inner(event.downcast::<E>().unwrap()));
            }));
    }

    fn emit(&mut self, context: &mut EventContext<'_>, event: Box<dyn Event>) {
        if let Some(handler) = self.handlers.get_mut(&event.as_ref().type_id()) {
            handler(context, event);
        }
    }
}

#[derive(Debug)]
struct EventContext<'a> {
    pub resources: &'a mut Resources,
    pub world: &'a mut World,
    pub rotator: &'a mut dyn Rotator,
    pub capture: &'a mut dyn Capture,
    pub map_service: &'a mut Box<dyn MapService>,
    pub character_service: &'a mut Box<dyn CharacterService>,
    pub rotator_service: &'a mut Box<dyn RotatorService>,
    pub capture_service: &'a mut Box<dyn CaptureService>,
    pub input_service: &'a mut Box<dyn InputService>,
    pub settings_service: &'a mut Box<dyn SettingsService>,
    pub localization_service: &'a mut Box<dyn LocalizationService>,
    pub operation_service: &'a mut Box<dyn OperationService>,
    pub mediator_service: &'a mut Box<dyn MediatorService>,
    #[cfg(debug_assertions)]
    pub debug_service: &'a mut DebugService,
}

#[derive(Debug)]
pub struct Services {
    event_bus: EventBus,
    event_rx: mpsc::UnboundedReceiver<Box<dyn Event>>,
    map: Box<dyn MapService>,
    character: Box<dyn CharacterService>,
    rotator: Box<dyn RotatorService>,
    capture: Box<dyn CaptureService>,
    input: Box<dyn InputService>,
    settings: Box<dyn SettingsService>,
    localization: Box<dyn LocalizationService>,
    operation: Box<dyn OperationService>,
    mediator: Box<dyn MediatorService>,
    #[cfg(debug_assertions)]
    debug: DebugService,
}

impl Services {
    pub fn new(
        settings: Rc<RefCell<Settings>>,
        localization: Rc<RefCell<Arc<Localization>>>,
        mut world_event_rx: Receiver<WorldEvent>,
    ) -> Self {
        let capture_service = DefaultCaptureService::new();
        let settings_service = DefaultSettingsService::new(settings);

        let mut rotator = DefaultRotatorService::default();
        rotator.update_from_settings(&settings_service.settings());

        let window = capture_service.selected_window();
        let input_rx = DefaultInputReceiver::new(window, InputKind::Focused);
        let input_service = DefaultInputService::new(input_rx);
        let mut input_event_rx = input_service.subscribe_event();

        let operation_service = DefaultOperationService::default();
        let mut operation_event_rx = operation_service.subscribe();

        let mut database_event_rx = database_event_receiver();
        let (mediator_service, mut mediator_event_rx) = DefaultMediatorService::new();

        let mut event_bus = EventBus {
            handlers: HashMap::default(),
        };
        event_bus.subscribe(MediatorEventHandler);
        event_bus.subscribe(DatabaseEventHandler);
        event_bus.subscribe(WorldEventHandler);
        event_bus.subscribe(OperationEventHandler);
        event_bus.subscribe(InputEventHandler);

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        spawn(async move {
            loop {
                let event: Box<dyn Event> = select! {
                    Some(event) = mediator_event_rx.recv() => Box::new(event),
                    Ok(event) = world_event_rx.recv() => Box::new(event),
                    Ok(event) = operation_event_rx.recv() => Box::new(event),
                    Ok(event) = input_event_rx.recv() => Box::new(event),
                    Ok(event) = database_event_rx.recv() => Box::new(event),
                };
                match event_tx.send(event) {
                    Ok(_) => (),
                    Err(err) => {
                        error!(target: "backend/services", "error when occured trying to send event {err}");
                        break;
                    }
                }
            }
        });

        Self {
            event_bus,
            event_rx,
            map: Box::new(DefaultMapService::default()),
            character: Box::new(DefaultCharacterService::default()),
            rotator: Box::new(rotator),
            capture: Box::new(capture_service),
            input: Box::new(input_service),
            settings: Box::new(settings_service),
            localization: Box::new(DefaultLocalizationService::new(localization)),
            operation: Box::new(operation_service),
            mediator: Box::new(mediator_service),
            #[cfg(debug_assertions)]
            debug: DebugService::default(),
        }
    }

    pub fn selected_window(&self) -> Window {
        self.capture.selected_window()
    }

    pub fn update_window(&mut self, input: &mut dyn Input, capture: &mut dyn Capture) {
        let settings = self.settings.settings();
        self.capture.apply_selected_window(capture);
        self.capture.apply_mode(capture, settings.capture_mode);

        let window = capture.window();
        self.input.apply_window(input, window);
        self.input
            .apply_method(input, InputMethod::from(&*settings));
    }

    #[inline]
    pub fn poll(
        &mut self,
        resources: &mut Resources,
        world: &mut World,
        rotator: &mut dyn Rotator,
        capture: &mut dyn Capture,
    ) {
        if let Ok(event) = self.event_rx.try_recv() {
            let mut context = EventContext {
                resources,
                world,
                rotator,
                capture,
                map_service: &mut self.map,
                character_service: &mut self.character,
                rotator_service: &mut self.rotator,
                capture_service: &mut self.capture,
                input_service: &mut self.input,
                settings_service: &mut self.settings,
                localization_service: &mut self.localization,
                operation_service: &mut self.operation,
                mediator_service: &mut self.mediator,
                #[cfg(debug_assertions)]
                debug_service: &mut self.debug,
            };
            debug!(target: "backend/services", "processing event {event:?}");
            self.event_bus.emit(&mut context, event);
        }

        #[cfg(debug_assertions)]
        self.debug.poll(resources);
        self.mediator.broadcast_state(resources, world);
    }
}
