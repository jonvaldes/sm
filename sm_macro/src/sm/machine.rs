use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    braced,
    parse::{Parse, ParseStream, Result},
    parse_quote, Ident,
};

use crate::sm::{
    event::{Event, Events},
    initial_state::InitialStates,
    state::{State, States},
    transition::Transitions,
};

#[derive(Debug, PartialEq)]
pub(crate) struct Machines(Vec<Machine>);

impl Parse for Machines {
    /// example machines tokens:
    ///
    /// ```text
    /// TurnStile { ... }
    /// Lock { ... }
    /// MyStateMachine { ... }
    /// ```
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut machines: Vec<Machine> = Vec::new();

        while !input.is_empty() {
            // `TurnStile { ... }`
            //  ^^^^^^^^^^^^^^^^^
            let machine = Machine::parse(input)?;
            machines.push(machine);
        }

        Ok(Machines(machines))
    }
}

impl ToTokens for Machines {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(quote! {
            use sm::{AsEnum, Initializer, Machine as M, Transition};
        });

        for machine in &self.0 {
            machine.to_tokens(tokens);
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Machine {
    pub name: Ident,
    pub initial_states: InitialStates,
    pub transitions: Transitions,
}

impl Machine {
    fn states(&self) -> States {
        let mut states: Vec<State> = Vec::new();

        for t in &self.transitions.0 {
            if !states.iter().any(|s| s.name == t.from.name) {
                states.push(t.from.clone());
            }

            if !states.iter().any(|s| s.name == t.to.name) {
                states.push(t.to.clone());
            }
        }

        for i in &self.initial_states.0 {
            if !states.iter().any(|s| s.name == i.name) {
                states.push(State {
                    name: i.name.clone(),
                });
            }
        }

        States(states)
    }

    fn events(&self) -> Events {
        let mut events: Vec<Event> = Vec::new();

        for t in &self.transitions.0 {
            if !events.iter().any(|s| s.name == t.event.name) {
                events.push(t.event.clone());
            }
        }

        Events(events)
    }
}

impl Parse for Machine {
    /// example machine tokens:
    ///
    /// ```text
    /// TurnStile {
    ///     InitialStates { ... }
    ///
    ///     Push { ... }
    ///     Coin { ... }
    /// }
    /// ```
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        // `TurnStile { ... }`
        //  ^^^^^^^^^
        let name: Ident = input.parse()?;

        // `TurnStile { ... }`
        //              ^^^
        let block_machine;
        braced!(block_machine in input);

        // `InitialStates { ... }`
        //  ^^^^^^^^^^^^^^^^^^^^^
        let initial_states = InitialStates::parse(&block_machine)?;

        // `Push { ... }`
        //  ^^^^^^^^^^^^
        let transitions = Transitions::parse(&block_machine)?;

        Ok(Machine {
            name,
            initial_states,
            transitions,
        })
    }
}

impl ToTokens for Machine {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let name = &self.name;
        let initial_states = &self.initial_states;
        let states = &self.states();
        let events = &self.events();
        let machine_enum = MachineEnum { machine: self };
        let transitions = &self.transitions;

        tokens.extend(quote! {
            #[allow(non_snake_case)]
            pub mod #name {
                use sm::{AsEnum, Event, InitialState, Initializer, Machine as M, NoneEvent, State, Transition};

                #[derive(Debug, Eq, PartialEq, Clone)]
                pub struct Machine<S: State, E: Event>(S, Option<E>);

                impl<S: State, E: Event> M for Machine<S, E> {
                    type State = S;
                    type Event = E;

                    fn state(&self) -> Self::State {
                        self.0.clone()
                    }

                    fn trigger(&self) -> Option<Self::Event> {
                        self.1.clone()
                    }
                }

                impl<S: InitialState> Initializer<S> for Machine<S, NoneEvent> {
                    type Machine = Machine<S, NoneEvent>;

                    fn new(state: S) -> Self::Machine {
                        Machine(state, Option::None)
                    }
                }

                #states
                #initial_states
                #events
                #machine_enum
                #transitions
            }
        });
    }
}

#[derive(Debug)]
#[allow(single_use_lifetimes)]
struct MachineEnum<'a> {
    machine: &'a Machine,
}

#[allow(single_use_lifetimes)]
impl<'a> ToTokens for MachineEnum<'a> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut variants = Vec::new();
        let mut states = Vec::new();
        let mut events = Vec::new();

        for s in &self.machine.initial_states.0 {
            let name = s.name.clone();
            let none = parse_quote! { NoneEvent };
            let variant = Ident::new(&format!("Initial{}", name), Span::call_site());

            variants.push(variant);
            states.push(name);
            events.push(none);
        }

        for t in &self.machine.transitions.0 {
            let state = t.to.name.clone();
            let event = t.event.name.clone();
            let variant = Ident::new(&format!("{}By{}", state, event), Span::call_site());

            if variants.contains(&variant) {
                continue;
            }

            variants.push(variant);
            states.push(state);
            events.push(event);
        }

        let variants = &variants;
        let states = &states;
        let events = &events;

        tokens.extend(quote! {
            #[derive(Debug, Clone)]
            pub enum Variant {
                #(#variants(Machine<#states, #events>)),*
            }

            #(
                impl AsEnum for Machine<#states, #events> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::#variants(self)
                    }
                }
            )*
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sm::{initial_state::InitialState, transition::Transition};
    use proc_macro2::TokenStream;
    use syn::{self, parse_quote};

    #[test]
    fn test_machine_parse() {
        let left: Machine = syn::parse2(quote! {
           TurnStile {
               InitialStates { Locked, Unlocked }

               Coin { Locked => Unlocked }
               Push { Unlocked => Locked }
           }
        })
        .unwrap();

        let right = Machine {
            name: parse_quote! { TurnStile },
            initial_states: InitialStates(vec![
                InitialState {
                    name: parse_quote! { Locked },
                },
                InitialState {
                    name: parse_quote! { Unlocked },
                },
            ]),
            transitions: Transitions(vec![
                Transition {
                    event: Event {
                        name: parse_quote! { Coin },
                    },
                    from: State {
                        name: parse_quote! { Locked },
                    },
                    to: State {
                        name: parse_quote! { Unlocked },
                    },
                },
                Transition {
                    event: Event {
                        name: parse_quote! { Push },
                    },
                    from: State {
                        name: parse_quote! { Unlocked },
                    },
                    to: State {
                        name: parse_quote! { Locked },
                    },
                },
            ]),
        };

        assert_eq!(left, right);
    }

    #[test]
    fn test_machine_to_tokens() {
        let machine = Machine {
            name: parse_quote! { TurnStile },
            initial_states: InitialStates(vec![
                InitialState {
                    name: parse_quote! { Unlocked },
                },
                InitialState {
                    name: parse_quote! { Locked },
                },
            ]),
            transitions: Transitions(vec![Transition {
                event: Event {
                    name: parse_quote! { Push },
                },
                from: State {
                    name: parse_quote! { Unlocked },
                },
                to: State {
                    name: parse_quote! { Locked },
                },
            }]),
        };

        let left = quote! {
            #[allow(non_snake_case)]
            mod TurnStile {
                use sm::{AsEnum, Event, InitialState, Initializer, Machine as M, NoneEvent, State, Transition};

                #[derive(Debug, Eq, PartialEq, Clone)]
                pub struct Machine<S: State, E: Event>(S, Option<E>);

                impl<S: State, E: Event> M for Machine<S, E> {
                    type State = S;
                    type Event = E;

                    fn state(&self) -> Self::State {
                        self.0.clone()
                    }

                    fn trigger(&self) -> Option<Self::Event> {
                        self.1.clone()
                    }
                }

                impl<S: InitialState> Initializer<S> for Machine<S, NoneEvent> {
                    type Machine = Machine<S, NoneEvent>;

                    fn new(state: S) -> Self::Machine {
                        Machine(state, Option::None)
                    }
                }

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Unlocked;
                impl State for Unlocked {}

                impl PartialEq<Unlocked> for Unlocked {
                    fn eq(&self, _: & Unlocked) -> bool {
                        true
                    }
                }

                impl PartialEq<Locked> for Unlocked {
                    fn eq(&self, _: & Locked) -> bool {
                        false
                    }
                }

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Locked;
                impl State for Locked {}

                impl PartialEq<Unlocked> for Locked {
                    fn eq(&self, _: &Unlocked) -> bool {
                        false
                    }
                }

                impl PartialEq<Locked> for Locked {
                    fn eq(&self, _: &Locked) -> bool {
                        true
                    }
                }

                impl InitialState for Unlocked {}
                impl InitialState for Locked {}

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Push;
                impl Event for Push {}

                impl PartialEq<Push> for Push {
                    fn eq(&self, _: &Push) -> bool {
                        true
                    }
                }

                #[derive(Debug, Clone)]
                pub enum Variant {
                    InitialUnlocked(Machine<Unlocked, NoneEvent>),
                    InitialLocked(Machine<Locked, NoneEvent>),
                    LockedByPush(Machine<Locked, Push>)
                }

                impl AsEnum for Machine<Unlocked, NoneEvent> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::InitialUnlocked(self)
                    }
                }

                impl AsEnum for Machine<Locked, NoneEvent> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::InitialLocked(self)
                    }
                }

                impl AsEnum for Machine<Locked, Push> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::LockedByPush(self)
                    }
                }

                impl<E: Event> Transition<Push> for Machine<Unlocked, E> {
                    type Machine = Machine<Locked, Push>;

                    fn transition(self, event: Push) -> Self::Machine {
                        Machine(Locked, Some(event))
                    }
                }
            }
        };

        let mut right = TokenStream::new();
        machine.to_tokens(&mut right);

        assert_eq!(format!("{}", left), format!("{}", right))
    }

    #[test]
    fn test_machines_parse() {
        let left: Machines = syn::parse2(quote! {
           TurnStile {
               InitialStates { Locked, Unlocked }

               Coin { Locked => Unlocked }
               Push { Unlocked => Locked }
           }

           Lock {
               InitialStates { Locked, Unlocked }

               TurnKey {
                   Locked => Unlocked
                   Unlocked => Locked
                }
           }
        })
        .unwrap();

        let right = Machines(vec![
            Machine {
                name: parse_quote! { TurnStile },
                initial_states: InitialStates(vec![
                    InitialState {
                        name: parse_quote! { Locked },
                    },
                    InitialState {
                        name: parse_quote! { Unlocked },
                    },
                ]),
                transitions: Transitions(vec![
                    Transition {
                        event: Event {
                            name: parse_quote! { Coin },
                        },
                        from: State {
                            name: parse_quote! { Locked },
                        },
                        to: State {
                            name: parse_quote! { Unlocked },
                        },
                    },
                    Transition {
                        event: Event {
                            name: parse_quote! { Push },
                        },
                        from: State {
                            name: parse_quote! { Unlocked },
                        },
                        to: State {
                            name: parse_quote! { Locked },
                        },
                    },
                ]),
            },
            Machine {
                name: parse_quote! { Lock },
                initial_states: InitialStates(vec![
                    InitialState {
                        name: parse_quote! { Locked },
                    },
                    InitialState {
                        name: parse_quote! { Unlocked },
                    },
                ]),
                transitions: Transitions(vec![
                    Transition {
                        event: Event {
                            name: parse_quote! { TurnKey },
                        },
                        from: State {
                            name: parse_quote! { Locked },
                        },
                        to: State {
                            name: parse_quote! { Unlocked },
                        },
                    },
                    Transition {
                        event: Event {
                            name: parse_quote! { TurnKey },
                        },
                        from: State {
                            name: parse_quote! { Unlocked },
                        },
                        to: State {
                            name: parse_quote! { Locked },
                        },
                    },
                ]),
            },
        ]);

        assert_eq!(left, right);
    }

    #[test]
    fn test_machines_to_tokens() {
        let machines = Machines(vec![
            Machine {
                name: parse_quote! { TurnStile },
                initial_states: InitialStates(vec![
                    InitialState {
                        name: parse_quote! { Locked },
                    },
                    InitialState {
                        name: parse_quote! { Unlocked },
                    },
                ]),
                transitions: Transitions(vec![
                    Transition {
                        event: Event {
                            name: parse_quote! { Coin },
                        },
                        from: State {
                            name: parse_quote! { Locked },
                        },
                        to: State {
                            name: parse_quote! { Unlocked },
                        },
                    },
                    Transition {
                        event: Event {
                            name: parse_quote! { Push },
                        },
                        from: State {
                            name: parse_quote! { Unlocked },
                        },
                        to: State {
                            name: parse_quote! { Locked },
                        },
                    },
                ]),
            },
            Machine {
                name: parse_quote! { Lock },
                initial_states: InitialStates(vec![
                    InitialState {
                        name: parse_quote! { Locked },
                    },
                    InitialState {
                        name: parse_quote! { Unlocked },
                    },
                ]),
                transitions: Transitions(vec![
                    Transition {
                        event: Event {
                            name: parse_quote! { TurnKey },
                        },
                        from: State {
                            name: parse_quote! { Locked },
                        },
                        to: State {
                            name: parse_quote! { Unlocked },
                        },
                    },
                    Transition {
                        event: Event {
                            name: parse_quote! { TurnKey },
                        },
                        from: State {
                            name: parse_quote! { Unlocked },
                        },
                        to: State {
                            name: parse_quote! { Locked },
                        },
                    },
                ]),
            },
        ]);

        let left = quote! {
            use sm::{AsEnum, Initializer, Machine as M, Transition};

            #[allow(non_snake_case)]
            mod TurnStile {
                use sm::{AsEnum, Event, InitialState, Initializer, Machine as M, NoneEvent, State, Transition};

                #[derive(Debug, Eq, PartialEq, Clone)]
                pub struct Machine<S: State, E: Event>(S, Option<E>);

                impl<S: State, E: Event> M for Machine<S, E> {
                    type State = S;
                    type Event = E;

                    fn state(&self) -> Self::State {
                        self.0.clone()
                    }

                    fn trigger(&self) -> Option<Self::Event> {
                        self.1.clone()
                    }
                }

                impl<S: InitialState> Initializer<S> for Machine<S, NoneEvent> {
                    type Machine = Machine<S, NoneEvent>;

                    fn new(state: S) -> Self::Machine {
                        Machine(state, Option::None)
                    }
                }

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Locked;
                impl State for Locked {}

                impl PartialEq<Locked> for Locked {
                    fn eq(&self, _: &Locked) -> bool {
                        true
                    }
                }

                impl PartialEq<Unlocked> for Locked {
                    fn eq(&self, _: &Unlocked) -> bool {
                        false
                    }
                }

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Unlocked;
                impl State for Unlocked {}

                impl PartialEq<Locked> for Unlocked {
                    fn eq(&self, _: & Locked) -> bool {
                        false
                    }
                }

                impl PartialEq<Unlocked> for Unlocked {
                    fn eq(&self, _: & Unlocked) -> bool {
                        true
                    }
                }

                impl InitialState for Locked {}
                impl InitialState for Unlocked {}

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Coin;
                impl Event for Coin {}

                impl PartialEq<Coin> for Coin {
                    fn eq(&self, _: &Coin) -> bool {
                        true
                    }
                }

                impl PartialEq<Push> for Coin {
                    fn eq(&self, _: &Push) -> bool {
                        false
                    }
                }

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Push;
                impl Event for Push {}

                impl PartialEq<Coin> for Push {
                    fn eq(&self, _: &Coin) -> bool {
                        false
                    }
                }

                impl PartialEq<Push> for Push {
                    fn eq(&self, _: &Push) -> bool {
                        true
                    }
                }

                #[derive(Debug, Clone)]
                pub enum Variant {
                    InitialLocked(Machine<Locked, NoneEvent>),
                    InitialUnlocked(Machine<Unlocked, NoneEvent>),
                    UnlockedByCoin(Machine<Unlocked, Coin>),
                    LockedByPush(Machine<Locked, Push>)
                }

                impl AsEnum for Machine<Locked, NoneEvent> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::InitialLocked(self)
                    }
                }

                impl AsEnum for Machine<Unlocked, NoneEvent> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::InitialUnlocked(self)
                    }
                }

                impl AsEnum for Machine<Unlocked, Coin> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::UnlockedByCoin(self)
                    }
                }

                impl AsEnum for Machine<Locked, Push> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::LockedByPush(self)
                    }
                }

                impl<E: Event> Transition<Coin> for Machine<Locked, E> {
                    type Machine = Machine<Unlocked, Coin>;

                    fn transition(self, event: Coin) -> Self::Machine {
                        Machine(Unlocked, Some(event))
                    }
                }

                impl<E: Event> Transition<Push> for Machine<Unlocked, E> {
                    type Machine = Machine<Locked, Push>;

                    fn transition(self, event: Push) -> Self::Machine {
                        Machine(Locked, Some(event))
                    }
                }
            }

            #[allow(non_snake_case)]
            mod Lock {
                use sm::{AsEnum, Event, InitialState, Initializer, Machine as M, NoneEvent, State, Transition};

                #[derive(Debug, Eq, PartialEq, Clone)]
                pub struct Machine<S: State, E: Event>(S, Option<E>);

                impl<S: State, E: Event> M for Machine<S, E> {
                    type State = S;
                    type Event = E;

                    fn state(&self) -> Self::State {
                        self.0.clone()
                    }

                    fn trigger(&self) -> Option<Self::Event> {
                        self.1.clone()
                    }
                }

                impl<S: InitialState> Initializer<S> for Machine<S, NoneEvent> {
                    type Machine = Machine<S, NoneEvent>;

                    fn new(state: S) -> Self::Machine {
                        Machine(state, Option::None)
                    }
                }

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Locked;
                impl State for Locked {}

                impl PartialEq<Locked> for Locked {
                    fn eq(&self, _: &Locked) -> bool {
                        true
                    }
                }

                impl PartialEq<Unlocked> for Locked {
                    fn eq(&self, _: &Unlocked) -> bool {
                        false
                    }
                }

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct Unlocked;
                impl State for Unlocked {}

                impl PartialEq<Locked> for Unlocked {
                    fn eq(&self, _: & Locked) -> bool {
                        false
                    }
                }

                impl PartialEq<Unlocked> for Unlocked {
                    fn eq(&self, _: & Unlocked) -> bool {
                        true
                    }
                }

                impl InitialState for Locked {}
                impl InitialState for Unlocked {}

                #[derive(Clone, Copy, Debug, Eq)]
                pub struct TurnKey;
                impl Event for TurnKey {}

                impl PartialEq<TurnKey> for TurnKey {
                    fn eq(&self, _: &TurnKey) -> bool {
                        true
                    }
                }

                #[derive(Debug, Clone)]
                pub enum Variant {
                    InitialLocked(Machine<Locked, NoneEvent>),
                    InitialUnlocked(Machine<Unlocked, NoneEvent>),
                    UnlockedByTurnKey(Machine<Unlocked, TurnKey>),
                    LockedByTurnKey(Machine<Locked, TurnKey>)
                }

                impl AsEnum for Machine<Locked, NoneEvent> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::InitialLocked(self)
                    }
                }

                impl AsEnum for Machine<Unlocked, NoneEvent> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::InitialUnlocked(self)
                    }
                }

                impl AsEnum for Machine<Unlocked, TurnKey> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::UnlockedByTurnKey(self)
                    }
                }

                impl AsEnum for Machine<Locked, TurnKey> {
                    type Enum = Variant;

                    fn as_enum(self) -> Self::Enum {
                        Variant::LockedByTurnKey(self)
                    }
                }
                impl<E: Event> Transition<TurnKey> for Machine<Locked, E> {
                    type Machine = Machine<Unlocked, TurnKey>;

                    fn transition(self, event: TurnKey) -> Self::Machine {
                        Machine(Unlocked, Some(event))
                    }
                }

                impl<E: Event> Transition<TurnKey> for Machine<Unlocked, E> {
                    type Machine = Machine<Locked, TurnKey>;

                    fn transition(self, event: TurnKey) -> Self::Machine {
                        Machine(Locked, Some(event))
                    }
                }
            }
        };

        let mut right = TokenStream::new();
        machines.to_tokens(&mut right);

        assert_eq!(format!("{}", left), format!("{}", right))
    }
}
