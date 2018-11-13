use std::collections::HashMap;
use std::error;
use std::fmt;

use {Name, Type, TypeSchema, Variable};

/// Errors during unification.
#[derive(Debug, Clone, PartialEq)]
pub enum UnificationError<N: Name = &'static str> {
    /// `Occurs` happens when occurs checks fail (i.e. a type variable is
    /// unified recursively). The id of the bad type variable is supplied.
    Occurs(Variable),
    /// `Failure` happens when symbols or type variants don't unify because of
    /// structural differences.
    Failure(Type<N>, Type<N>),
}
impl<N: Name> fmt::Display for UnificationError<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            UnificationError::Occurs(v) => write!(f, "Occurs({})", v),
            UnificationError::Failure(ref t1, ref t2) => {
                write!(f, "Failure({}, {})", t1.show(false), t2.show(false))
            }
        }
    }
}
impl<N: Name + fmt::Debug> error::Error for UnificationError<N> {
    fn description(&self) -> &'static str {
        "unification failed"
    }
}

/// A type environment. Useful for reasoning about [`Type`]s (e.g unification,
/// type inference).
///
/// Contexts track substitutions and generate fresh type variables.
///
/// [`Type`]: enum.Type.html
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context<N: Name = &'static str> {
    pub(crate) substitution: HashMap<Variable, Type<N>>,
    next: Variable,
}
impl<N: Name> Default for Context<N> {
    fn default() -> Self {
        Context {
            substitution: HashMap::new(),
            next: 0,
        }
    }
}
impl<N: Name> Context<N> {
    /// The substitution managed by the context.
    pub fn substitution(&self) -> &HashMap<Variable, Type<N>> {
        &self.substitution
    }
    /// Create a new substitution for [`Type::Variable`] number `v` to the
    /// [`Type`] `t`.
    ///
    /// [`Type`]: enum.Type.html
    /// [`Type::Variable`]: enum.Type.html#variant.Variable
    pub fn extend(&mut self, v: Variable, t: Type<N>) {
        if v >= self.next {
            self.next = v + 1
        }
        self.substitution.insert(v, t);
    }
    /// Create a new [`Type::Variable`] from the next unused number.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::{Type, Context};
    /// let mut ctx = Context::default();
    ///
    /// // Get a fresh variable
    /// let t0 = ctx.new_variable();
    /// assert_eq!(t0, Type::Variable(0));
    ///
    /// // Instantiating a polytype will yield new variables
    /// let t = ptp!(0, 1; @arrow[tp!(0), tp!(1), tp!(1)]);
    /// let t = t.instantiate(&mut ctx);
    /// assert_eq!(t.to_string(), "t1 → t2 → t2");
    ///
    /// // Get another fresh variable
    /// let t3 = ctx.new_variable();
    /// assert_eq!(t3, Type::Variable(3));
    /// # }
    /// ```
    ///
    /// [`Type::Variable`]: enum.Type.html#variant.Variable
    pub fn new_variable(&mut self) -> Type<N> {
        self.next += 1;
        Type::Variable(self.next - 1)
    }
    /// Create constraints within the context that ensure `t1` and `t2`
    /// unify.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::Context;
    /// let mut ctx = Context::default();
    ///
    /// let t1 = tp!(@arrow[tp!(int), tp!(0)]);
    /// let t2 = tp!(@arrow[tp!(1), tp!(bool)]);
    /// ctx.unify(&t1, &t2).expect("unifies");
    ///
    /// let t1 = t1.apply(&ctx);
    /// let t2 = t2.apply(&ctx);
    /// assert_eq!(t1, t2);  // int → bool
    /// # }
    /// ```
    ///
    /// Unification errors leave the context unaffected. A
    /// [`UnificationError::Failure`] error happens when symbols don't match:
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::{Context, UnificationError};
    /// let mut ctx = Context::default();
    ///
    /// let t1 = tp!(@arrow[tp!(int), tp!(0)]);
    /// let t2 = tp!(@arrow[tp!(bool), tp!(1)]);
    /// let res = ctx.unify(&t1, &t2);
    ///
    /// if let Err(UnificationError::Failure(left, right)) = res {
    ///     // failed to unify t1 with t2.
    ///     assert_eq!(left, tp!(int));
    ///     assert_eq!(right, tp!(bool));
    /// } else { unreachable!() }
    /// # }
    /// ```
    ///
    /// An [`UnificationError::Occurs`] error happens when the same type
    /// variable occurs in both types in a circular way. Ensure you
    /// [`instantiate`][] your types properly, so type variables don't overlap
    /// unless you mean them to.
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::{Context, UnificationError};
    /// let mut ctx = Context::default();
    ///
    /// let t1 = tp!(1);
    /// let t2 = tp!(@arrow[tp!(bool), tp!(1)]);
    /// let res = ctx.unify(&t1, &t2);
    ///
    /// if let Err(UnificationError::Occurs(v)) = res {
    ///     // failed to unify t1 with t2 because of circular type variable occurrence.
    ///     // t1 would have to be bool -> bool -> ... ad infinitum.
    ///     assert_eq!(v, 1);
    /// } else { unreachable!() }
    /// # }
    /// ```
    ///
    /// [`UnificationError::Failure`]: enum.UnificationError.html#variant.Failure
    /// [`UnificationError::Occurs`]: enum.UnificationError.html#variant.Occurs
    /// [`instantiate`]: enum.Type.html#method.instantiate
    pub fn unify(&mut self, t1: &Type<N>, t2: &Type<N>) -> Result<(), UnificationError<N>> {
        let mut t1 = t1.clone();
        let mut t2 = t2.clone();
        t1.apply_mut(self);
        t2.apply_mut(self);
        let mut ctx = self.clone();
        ctx.unify_internal(t1, t2)?;
        *self = ctx;
        Ok(())
    }
    /// Like [`unify`], but may affect the context even under failure. Hence, use this if you
    /// discard the context upon failure.
    ///
    /// [`unify`]: #method.unify
    pub fn unify_fast(
        &mut self,
        mut t1: Type<N>,
        mut t2: Type<N>,
    ) -> Result<(), UnificationError<N>> {
        t1.apply_mut(self);
        t2.apply_mut(self);
        self.unify_internal(t1, t2)
    }
    /// unify_internal may mutate the context even with an error. The context on
    /// which it's called should be discarded if there's an error.
    fn unify_internal(&mut self, t1: Type<N>, t2: Type<N>) -> Result<(), UnificationError<N>> {
        if t1 == t2 {
            return Ok(());
        }
        match (t1, t2) {
            (Type::Variable(v), t2) => {
                if t2.occurs(v) {
                    Err(UnificationError::Occurs(v))
                } else {
                    self.extend(v, t2.clone());
                    Ok(())
                }
            }
            (t1, Type::Variable(v)) => {
                if t1.occurs(v) {
                    Err(UnificationError::Occurs(v))
                } else {
                    self.extend(v, t1.clone());
                    Ok(())
                }
            }
            (Type::Constructed(n1, a1), Type::Constructed(n2, a2)) => {
                if n1 != n2 {
                    Err(UnificationError::Failure(
                        Type::Constructed(n1, a1),
                        Type::Constructed(n2, a2),
                    ))
                } else {
                    for (mut t1, mut t2) in a1.into_iter().zip(a2) {
                        t1.apply_mut(self);
                        t2.apply_mut(self);
                        self.unify_internal(t1, t2)?;
                    }
                    Ok(())
                }
            }
        }
    }
    /// Confines the substitution to those which act on the given variables.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # fn main() {
    /// # use polytype::Context;
    /// let mut ctx = Context::default();
    /// let v0 = ctx.new_variable();
    /// let v1 = ctx.new_variable();
    /// ctx.unify(&v0, &tp!(int));
    /// ctx.unify(&v1, &tp!(bool));
    ///
    /// {
    ///     let sub = ctx.substitution();
    ///     assert_eq!(sub.len(), 2);
    ///     assert_eq!(sub[&0], tp!(int));
    ///     assert_eq!(sub[&1], tp!(bool));
    /// }
    ///
    /// // confine the substitution to v1
    /// ctx.confine(&[1]);
    /// let sub = ctx.substitution();
    /// assert_eq!(sub.len(), 1);
    /// assert_eq!(sub[&1], tp!(bool));
    /// # }
    /// ```
    pub fn confine(&mut self, keep: &[Variable]) {
        let mut substitution = HashMap::new();
        for v in keep {
            substitution.insert(*v, self.substitution[v].clone());
        }
        self.substitution = substitution;
    }
    /// Merge two type contexts.
    ///
    /// Every [`Type`] ([`TypeSchema`]) that corresponds to the `other` context
    /// must be reified using [`ContextChange::reify_type`]
    /// ([`ContextChange::reify_typeschema`]). Any [`Variable`] in `sacreds`
    /// will not be changed by the context (i.e. reification will ignore it).
    ///
    /// # Examples
    ///
    /// Without sacred variables, which assumes that all type variables between the contexts are
    /// distinct:
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # use polytype::{Type, Context};
    /// # fn main() {
    /// let mut ctx = Context::default();
    /// let a = ctx.new_variable();
    /// let b = ctx.new_variable();
    /// ctx.unify(&Type::arrow(a, b), &tp!(@arrow[tp!(int), tp!(bool)])).unwrap();
    /// // ctx uses t0 and t1
    ///
    /// let mut ctx2 = Context::default();
    /// let pt = ptp!(0, 1; @arrow[tp!(0), tp!(1)]);
    /// let mut t = pt.instantiate(&mut ctx2);
    /// ctx2.extend(0, tp!(bool));
    /// assert_eq!(t.apply(&ctx2).to_string(), "bool → t1");
    /// // ctx2 uses t0 and t1
    ///
    /// let ctx_change = ctx.merge(ctx2, vec![]);
    /// // rewrite all terms under ctx2 using ctx_change
    /// ctx_change.reify_type(&mut t);
    /// assert_eq!(t.to_string(), "t2 → t3");
    /// assert_eq!(t.apply(&ctx).to_string(), "bool → t3");
    ///
    /// assert_eq!(ctx.new_variable(), tp!(4));
    /// # }
    /// ```
    ///
    /// With sacred variables, which specifies which type variables are equivalent in both
    /// contexts:
    ///
    /// ```
    /// # #[macro_use] extern crate polytype;
    /// # use polytype::{Type, Context};
    /// # fn main() {
    /// let mut ctx = Context::default();
    /// let a = ctx.new_variable();
    /// let b = ctx.new_variable();
    /// ctx.unify(&Type::arrow(a, b), &tp!(@arrow[tp!(int), tp!(bool)])).unwrap();
    /// // ctx uses t0 and t1
    ///
    /// let mut ctx2 = Context::default();
    /// let a = ctx2.new_variable();
    /// let b = ctx2.new_variable();
    /// let mut t = Type::arrow(a, b);
    /// ctx2.extend(0, tp!(bool));
    /// assert_eq!(t.apply(&ctx2).to_string(), "bool → t1");
    /// // ctx2 uses t0 and t1
    ///
    /// // t1 from ctx2 is preserved *and* constrained by ctx
    /// let ctx_change = ctx.merge(ctx2, vec![1]);
    /// // rewrite all terms under ctx2 using ctx_change
    /// ctx_change.reify_type(&mut t);
    /// assert_eq!(t.to_string(), "t2 → t1");
    /// assert_eq!(t.apply(&ctx).to_string(), "bool → bool");
    ///
    /// assert_eq!(ctx.new_variable(), tp!(4));
    /// # }
    /// ```
    /// [`ContextChange::reify_type`]: struct.ContextChange.html#method.reify_type
    /// [`ContextChange::reify_typeschema`]: struct.ContextChange.html#method.reify_typeschema
    /// [`Type`]: enum.Type.html
    /// [`TypeSchema`]: enum.TypeSchema.html
    /// [`Variable`]: type.TypeSchema.html
    pub fn merge(&mut self, other: Context<N>, sacreds: Vec<Variable>) -> ContextChange {
        let delta = self.next;
        for (v, tp) in other.substitution {
            self.substitution.insert(delta + v, tp);
        }
        // this is intentionally wasting variable space when there are sacreds:
        self.next += other.next;
        ContextChange { delta, sacreds }
    }

    /// Remove detours in substitution table
    pub fn reduct_substitution(&mut self) {
        let mut ret = HashMap::new();
        for (k, v) in &self.substitution {
            let mut v = v;
            while let Type::Variable(k2) = v {
                if let Some(v2) = self.substitution.get(&k2) {
                    v = v2;
                } else {
                    panic!("type not resolved in subst reduction")
                }
            }
            ret.insert(*k, v.clone());
        }
        self.substitution = ret;
    }
}

/// Allow types to be reified for use in a different context. See [`Context::merge`].
///
/// [`Context::merge`]: struct.Context.html#method.merge
pub struct ContextChange {
    delta: u16,
    sacreds: Vec<Variable>,
}
impl ContextChange {
    /// Reify a [`Type`] for use under a merged [`Context`].
    ///
    /// [`Type`]: enum.Type.html
    /// [`Context`]: struct.Context.html
    pub fn reify_type(&self, tp: &mut Type) {
        match tp {
            Type::Constructed(_, args) => for arg in args {
                self.reify_type(arg)
            },
            Type::Variable(n) if self.sacreds.contains(n) => (),
            Type::Variable(n) => *n += self.delta,
        }
    }
    /// Reify a [`TypeSchema`] for use under a merged [`Context`].
    ///
    /// [`TypeSchema`]: enum.TypeSchema.html
    /// [`Context`]: struct.Context.html
    pub fn reify_typeschema(&self, tpsc: &mut TypeSchema) {
        match tpsc {
            TypeSchema::Monotype(tp) => self.reify_type(tp),
            TypeSchema::Polytype { variable, body } => {
                *variable += self.delta;
                self.reify_typeschema(body);
            }
        }
    }
}
