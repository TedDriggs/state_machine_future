//! AST types for state machines and their states.

use darling;
use phases;
use syn;

use std::collections::HashSet;

/// A description of a state machine: its various states, which is the start
/// state, ready state, and error state.
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(state_machine_future), supports(enum_any), forward_attrs(allow, cfg))]
pub struct StateMachine<P: phases::Phase> {
    pub ident: syn::Ident,
    pub vis: syn::Visibility,
    pub generics: syn::Generics,
    pub data: darling::ast::Data<State<P>, ()>,
    pub attrs: Vec<syn::Attribute>,

    /// I guess we can't get other derives into `attrs` so we have to create our
    /// own derive list.
    #[darling(default)]
    pub derive: darling::util::IdentList,

    /// Extra per-phase data.
    #[darling(default)]
    pub extra: P::StateMachineExtra,
}

/// In individual state in a state machine.
#[derive(Debug, FromVariant)]
#[darling(attributes(state_machine_future, transitions, start, ready, error),
          forward_attrs(allow, doc, cfg))]
pub struct State<P: phases::Phase> {
    pub ident: syn::Ident,
    pub attrs: Vec<syn::Attribute>,
    pub fields: darling::ast::Fields<syn::Field>,

    /// Whether this is the start state.
    #[darling(default)]
    pub start: bool,

    /// Whether this is the ready state.
    #[darling(default)]
    pub ready: bool,

    /// Whether this is the error state.
    #[darling(default)]
    pub error: bool,

    /// The set of other states that this one can transition to.
    #[darling(default)]
    pub transitions: darling::util::IdentList,

    /// Any extra per-phase data.
    #[darling(default)]
    pub extra: P::StateExtra,
}

impl<P> StateMachine<P>
where
    P: phases::Phase,
{
    /// Split this state machine into its parts, and then create a state machine
    /// in another phase.
    pub fn and_then<F, Q>(self, mut f: F) -> StateMachine<Q>
    where
        Q: phases::Phase,
        F: FnMut(StateMachine<phases::NoPhase>, P::StateMachineExtra, Vec<State<P>>)
            -> StateMachine<Q>,
    {
        let (state_machine, extra, states) = self.split();
        f(state_machine, extra, states)
    }

    /// Split this state machine into its parts, separating per-phase data from
    /// the state machine.
    pub fn split(
        self,
    ) -> (
        StateMachine<phases::NoPhase>,
        P::StateMachineExtra,
        Vec<State<P>>,
    ) {
        let states = self.data.take_enum().unwrap();
        let extra = self.extra;
        let machine = StateMachine {
            ident: self.ident,
            vis: self.vis,
            generics: self.generics,
            data: darling::ast::Data::Enum(vec![]),
            attrs: self.attrs,
            derive: self.derive,
            extra: (),
        };
        (machine, extra, states)
    }

    /// Get this state machine's states.
    pub fn states(&self) -> &[State<P>] {
        match self.data {
            darling::ast::Data::Enum(ref states) => states,
            darling::ast::Data::Struct(_) => unreachable!(),
        }
    }
}

impl StateMachine<phases::NoPhase> {
    /// Join the state machine with the new phase's extra data, creating a state
    /// machine in the new phase.
    pub fn join<P>(self, extra: P::StateMachineExtra, states: Vec<State<P>>) -> StateMachine<P>
    where
        P: phases::Phase,
    {
        StateMachine {
            ident: self.ident,
            vis: self.vis,
            generics: self.generics,
            data: darling::ast::Data::Enum(states),
            attrs: self.attrs,
            derive: self.derive,
            extra,
        }
    }
}

impl<P> State<P>
where
    P: phases::Phase,
{
    /// Split this state into its parts, and then construct a state in some new
    /// phase.
    pub fn and_then<F, Q>(self, mut f: F) -> State<Q>
    where
        F: FnMut(State<phases::NoPhase>, P::StateExtra) -> State<Q>,
        Q: phases::Phase,
    {
        let (state, extra) = self.split();
        f(state, extra)
    }

    /// Split this state into its parts, separating its per-phase data out.
    pub fn split(self) -> (State<phases::NoPhase>, P::StateExtra) {
        let extra = self.extra;
        let state = State {
            ident: self.ident,
            attrs: self.attrs,
            fields: self.fields,
            start: self.start,
            ready: self.ready,
            error: self.error,
            transitions: self.transitions,
            extra: (),
        };
        (state, extra)
    }
}

pub trait CollectIdents {
    /// Collects idents that could be a type/lifetime parameter
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>);
}

impl CollectIdents for syn::Type {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        match *self {
            syn::Type::Path(ref expr_path) => {
                if let Some(ref qself) = expr_path.qself {
                    qself.ty.collect_idents(idents);
                }

                expr_path.path.collect_idents(idents);
            }
            syn::Type::Slice(ref ty) => ty.elem.collect_idents(idents),
            syn::Type::Paren(ref ty) => ty.elem.collect_idents(idents),
            syn::Type::Ptr(ref ty) => ty.elem.collect_idents(idents),
            syn::Type::Reference(ref reference) => {
                if let Some(ref lifetime) = reference.lifetime {
                    lifetime.collect_idents(idents);
                }

                reference.elem.collect_idents(idents)
            }
            syn::Type::Tuple(ref tys) => tys.elems.iter().for_each(|v| v.collect_idents(idents)),
            syn::Type::BareFn(ref bfn) => bfn.collect_idents(idents),
            syn::Type::Array(ref ty) => {
                ty.elem.collect_idents(idents);
                // ty.len.collect_idents(idents);
            }
            syn::Type::Never(_)
            | syn::Type::Macro(_)
            | syn::Type::TraitObject(_)
            | syn::Type::ImplTrait(_)
            | syn::Type::Infer(_) => {}
        }
    }
}

impl CollectIdents for syn::Expr {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        match *self {
            syn::Expr::Call(ref expr) => {
                expr.func.collect_idents(idents);
                expr.args.iter().for_each(|v| v.collect_idents(idents));
            }
            syn::Expr::Binary(ref expr) => {
                expr.left.collect_idents(idents);
                expr.right.collect_idents(idents);
            }
            syn::Expr::Index(ref expr) => {
                expr.expr.collect_idents(idents);
                expr.index.collect_idents(idents);
            }
            syn::Expr::Unary(ref expr) => {
                expr.expr.collect_idents(idents)
            }
            syn::Expr::Paren(ref expr) => {
                expr.expr.collect_idents(idents);
            }
            syn::Expr::Cast(ref expr) => {
                expr.ty.collect_idents(idents);
                expr.expr.collect_idents(idents);
            }
            syn::Expr::Path(ref p) => p.path.collect_idents(idents),
            syn::Expr::Lit(_) => {}
        }
    }
}

impl CollectIdents for syn::TypeBareFn {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        self.inputs.iter().for_each(|v| v.ty.collect_idents(idents));

        match self.output {
            syn::ReturnType::Type(_, ref ty) => ty.collect_idents(idents),
            syn::ReturnType::Default => {}
        }
    }
}

impl CollectIdents for syn::Path {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        // If the path contains only one segment and is not a global path,
        // it could be a generic type parameter, so we add the ident.
        if self.segments.len() == 1 && !self.global() {
            let last = self.segments.iter().next().unwrap();
            idents.insert(last.ident.clone());
        }

        // If the path has more than one segment, it can not be a type parameter, because type
        // parameters are absolute without any preceding segments. So, only collect
        // the idents of the path parameters (aka type/lifetime parameters).
        self.segments
            .iter()
            .for_each(|s| s.arguments.collect_idents(idents));
    }
}

impl CollectIdents for syn::PathArguments {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        match *self {
            syn::PathArguments::AngleBracketed(ref bracket) => {
                bracket.args.iter().for_each(|v| {
                    match v {
                        syn::GenericArgument::Lifetime(w) => w.collect_idents(idents),
                        syn::GenericArgument::Type(w) => w.collect_idents(idents),
                        syn::GenericArgument::Binding(w) => w.ty.collect_idents(idents),
                        syn::GenericArgument::Const(w) => {}
                    };
                });
            }
            syn::PathArguments::Parenthesized(ref parent) => {
                parent.inputs.iter().for_each(|v| v.collect_idents(idents));

                if let syn::ReturnType::Type(_, ref output) = parent.output {
                    output.collect_idents(idents);
                }
            }
        }
    }
}

impl CollectIdents for syn::Lifetime {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        unimplemented!()
    }
}

impl CollectIdents for syn::TypeParamBound {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        match *self {
            syn::TypeParamBound::Trait(ref poly) => {
                poly.lifetimes.iter().for_each(|l| {
                    l.lifetimes.iter().for_each(|l| l.collect_idents(idents));
                });

                poly.path.collect_idents(idents);
            }
            syn::TypeParamBound::Lifetime(ref lifetime) => {
                lifetime.collect_idents(idents);
            }
        }
    }
}

impl CollectIdents for syn::TypeParam {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        if let Some(ref default) = self.default {
            default.collect_idents(idents);
        }

        self.bounds.iter().for_each(|b| b.collect_idents(idents));
        idents.insert(self.ident.clone());
    }
}

impl CollectIdents for syn::LifetimeDef {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        self.bounds.iter().for_each(|b| {
            b.collect_idents(idents);
        });
        self.lifetime.collect_idents(idents);
    }
}

impl CollectIdents for syn::WherePredicate {
    fn collect_idents(&self, idents: &mut HashSet<syn::Ident>) {
        match *self {
            syn::WherePredicate::Type(ref bound) => {
                if let Some(ref bound_lifetimes) = bound.lifetimes {
                    bound_lifetimes
                        .lifetimes
                        .iter()
                        .for_each(|b| b.collect_idents(idents));
                }

                bound.bounds.iter().for_each(|b| b.collect_idents(idents));
                bound.bounded_ty.collect_idents(idents);
            }
            syn::WherePredicate::Lifetime(ref region) => {
                region.bounds.iter().for_each(|l| {
                    l.collect_idents(idents);
                });
                region.lifetime.collect_idents(idents);
            }
            syn::WherePredicate::Eq(ref eq) => {
                eq.lhs_ty.collect_idents(idents);
                eq.rhs_ty.collect_idents(idents);
            }
        }
    }
}

impl State<phases::NoPhase> {
    /// Join the state with the new phase's extra data, creating a state in the
    /// new phase.
    pub fn join<P>(self, extra: P::StateExtra) -> State<P>
    where
        P: phases::Phase,
    {
        State {
            ident: self.ident,
            attrs: self.attrs,
            fields: self.fields,
            start: self.start,
            ready: self.ready,
            error: self.error,
            transitions: self.transitions,
            extra,
        }
    }
}
