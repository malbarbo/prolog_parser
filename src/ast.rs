use rug::{Integer, Rational};
use ordered_float::*;
use crate::string_list::*;
use crate::tabled_rc::*;

use crate::put_back_n::*;

use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::{Bytes, Error as IOError, Read};
use std::rc::Rc;
use std::str::Utf8Error;
use std::vec::Vec;

pub type Atom = String;

pub type Var = String;

pub type Specifier = u32;

pub const MAX_ARITY: usize = 63;

pub const XFX: u32 = 0x0001;
pub const XFY: u32 = 0x0002;
pub const YFX: u32 = 0x0004;
pub const XF: u32  = 0x0010;
pub const YF: u32  = 0x0020;
pub const FX: u32  = 0x0040;
pub const FY: u32  = 0x0080;
pub const DELIMITER: u32 = 0x0100;
pub const TERM: u32  = 0x1000;
pub const LTERM: u32 = 0x3000;

pub const NEGATIVE_SIGN: u32 = 0x0200;

#[macro_export]
macro_rules! clause_name {
    ($name: expr, $tbl: expr) => (
        ClauseName::User(TabledRc::new($name, $tbl.clone()))
    ) ;
    ($name: expr) => (
        ClauseName::BuiltIn($name)
    )
}

#[macro_export]
macro_rules! atom {
    ($e:expr, $tbl:expr) => (
        Constant::Atom(ClauseName::User(tabled_rc!($e, $tbl)), None)
    );
    ($e:expr) => (
        Constant::Atom(clause_name!($e), None)
    )
}

#[macro_export]
macro_rules! rc_atom {
    ($e:expr) => (
        Rc::new(String::from($e))
    )
}
macro_rules! is_term {
    ($x:expr) => ( ($x & TERM) != 0 )
}

macro_rules! is_lterm {
    ($x:expr) => ( ($x & LTERM) != 0 )
}

macro_rules! is_op {
    ($x:expr) => ( $x & (XF | YF | FX | FY | XFX | XFY | YFX) != 0 )
}

macro_rules! is_negate {
    ($x:expr) => ( ($x & NEGATIVE_SIGN) != 0 )
}

#[macro_export]
macro_rules! is_prefix {
    ($x:expr) => ( $x & (FX | FY) != 0 )
}

#[macro_export]
macro_rules! is_postfix {
    ($x:expr) => ( $x & (XF | YF) != 0 )
}

#[macro_export]
macro_rules! is_infix {
    ($x:expr) => ( ($x & (XFX | XFY | YFX)) != 0 )
}

#[macro_export]
macro_rules! is_xfx {
    ($x:expr) => ( ($x & XFX) != 0 )
}

#[macro_export]
macro_rules! is_xfy {
    ($x:expr) => ( ($x & XFY) != 0 )
}

#[macro_export]
macro_rules! is_yfx {
    ($x:expr) => ( ($x & YFX) != 0 )
}

#[macro_export]
macro_rules! is_yf {
    ($x:expr) => ( ($x & YF) != 0 )
}

#[macro_export]
macro_rules! is_xf {
    ($x:expr) => ( ($x & XF) != 0 )
}

#[macro_export]
macro_rules! is_fx {
    ($x:expr) => ( ($x & FX) != 0 )
}

#[macro_export]
macro_rules! is_fy {
    ($x:expr) => ( ($x & FY) != 0 )
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegType {
    Perm(usize),
    Temp(usize)
}

impl Default for RegType {
    fn default() -> Self {
        RegType::Temp(0)
    }
}

impl RegType {
    pub fn reg_num(self) -> usize {
        match self {
            RegType::Perm(reg_num) | RegType::Temp(reg_num) => reg_num
        }
    }

    pub fn is_perm(self) -> bool {
        match self {
            RegType::Perm(_) => true,
            _ => false
        }
    }
}

impl fmt::Display for RegType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &RegType::Perm(val) => write!(f, "Y{}", val),
            &RegType::Temp(val) => write!(f, "X{}", val)
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum VarReg {
    ArgAndNorm(RegType, usize),
    Norm(RegType)
}

impl VarReg {
    pub fn norm(self) -> RegType {
        match self {
            VarReg::ArgAndNorm(reg, _) | VarReg::Norm(reg) => reg
        }
    }
}

impl fmt::Display for VarReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &VarReg::Norm(RegType::Perm(reg)) => write!(f, "Y{}", reg),
            &VarReg::Norm(RegType::Temp(reg)) => write!(f, "X{}", reg),
            &VarReg::ArgAndNorm(RegType::Perm(reg), arg) =>
                write!(f, "Y{} A{}", reg, arg),
            &VarReg::ArgAndNorm(RegType::Temp(reg), arg) =>
                write!(f, "X{} A{}", reg, arg)
        }
    }
}

impl Default for VarReg {
    fn default() -> Self {
        VarReg::Norm(RegType::default())
    }
}

#[macro_export]
macro_rules! temp_v {
    ($x:expr) => (
        RegType::Temp($x)
    )
}

#[macro_export]
macro_rules! perm_v {
    ($x:expr) => (
        RegType::Perm($x)
    )
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenContext {
    Head, Mid(usize), Last(usize) // Mid & Last: chunk_num
}

impl GenContext {
    pub fn chunk_num(self) -> usize {
        match self {
            GenContext::Head => 0,
            GenContext::Mid(cn) | GenContext::Last(cn) => cn
        }
    }
}

pub type OpDirKey = (ClauseName, Fixity);

#[derive(Clone)]
pub struct OpDirValue(pub SharedOpDesc, pub ClauseName);

impl OpDirValue {
    pub fn new(spec: Specifier, priority: usize, module_name: ClauseName) -> Self {
        OpDirValue(SharedOpDesc::new(priority, spec), module_name)
    }

    #[inline]
    pub fn shared_op_desc(&self) -> SharedOpDesc {
        self.0.clone()
    }

    #[inline]
    pub fn owning_module(&self) -> ClauseName {
        self.1.clone()
    }
}

// name and fixity -> operator type and precedence.
pub type OpDir = HashMap<OpDirKey, OpDirValue>;

#[derive(Clone, Copy)]
pub struct MachineFlags {
    pub double_quotes: DoubleQuotes
}

impl Default for MachineFlags {
    fn default() -> Self {
        MachineFlags { double_quotes: DoubleQuotes::default() }
    }
}

#[derive(Clone, Copy)]
pub enum DoubleQuotes {
    Atom, Chars, Codes
}

impl DoubleQuotes {
    pub fn is_chars(self) -> bool {
        if let DoubleQuotes::Chars = self {
            true
        } else {
            false
        }
    }

    pub fn is_atom(self) -> bool {
        if let DoubleQuotes::Atom = self {
            true
        } else {
            false
        }
    }

    pub fn is_codes(self) -> bool {
        if let DoubleQuotes::Codes = self {
            true
        } else {
            false
        }
    }
}

impl Default for DoubleQuotes {
    fn default() -> Self {
        DoubleQuotes::Chars
    }
}

pub fn default_op_dir() -> OpDir {
    let module_name = clause_name!("builtins");
    let mut op_dir = OpDir::new();

    op_dir.insert((clause_name!(":-"), Fixity::In),  OpDirValue::new(XFX, 1200, module_name.clone()));
    op_dir.insert((clause_name!(":-"), Fixity::Pre), OpDirValue::new(FX,  1200, module_name.clone()));
    op_dir.insert((clause_name!("?-"), Fixity::Pre), OpDirValue::new(FX,  1200, module_name.clone()));
    op_dir.insert((clause_name!(","), Fixity::In),   OpDirValue::new(XFY, 1000, module_name.clone()));

    op_dir
}

#[derive(Clone)]
pub enum ArithmeticError {
    NonEvaluableFunctor(Constant, usize),
    UninstantiatedVar
}

pub enum ParserError {
    Arithmetic(ArithmeticError),
    BackQuotedString,
    BadPendingByte,
    CannotParseCyclicTerm,
    UnexpectedChar(char),
    UnexpectedEOF,
    IO(IOError),
    ExpectedRel,
    ExpectedTopLevelTerm,
    InadmissibleFact,
    InadmissibleQueryTerm,
    IncompleteReduction,
    InconsistentEntry,
    InvalidHook,
    InvalidModuleDecl,
    InvalidModuleExport,
    InvalidRuleHead,
    InvalidUseModuleDecl,
    InvalidModuleResolution,
    InvalidSingleQuotedCharacter,
    MissingQuote,
    NonPrologChar,
    ParseBigInt,
    ParseFloat,
    Utf8Error
}

impl ParserError {
    pub fn as_str(&self) -> &'static str {
        match self {
            &ParserError::Arithmetic(_) =>
                "arithmetic_error",
            &ParserError::BackQuotedString =>
                "back_quoted_string",
            &ParserError::BadPendingByte =>
                "bad_pending_byte",
            &ParserError::UnexpectedChar(_) =>
                "unexpected_char",
            &ParserError::UnexpectedEOF =>
                "unexpected_end_of_file",
            &ParserError::ExpectedRel =>
                "expected_relation",
            &ParserError::ExpectedTopLevelTerm =>
                "expected_atom_or_cons_or_clause",
            &ParserError::InadmissibleFact =>
                "inadmissible_fact",
            &ParserError::InadmissibleQueryTerm =>
                "inadmissible_query_term",
            &ParserError::IncompleteReduction =>
                "incomplete_reduction",
            &ParserError::InconsistentEntry =>
                "inconsistent_entry",
            &ParserError::InvalidHook =>
                "invalid_hook",
            &ParserError::InvalidModuleDecl =>
                "invalid_module_declaration",
            &ParserError::InvalidModuleExport =>
                "invalid_module_export",
            &ParserError::InvalidModuleResolution =>
                "invalid_module_resolution",
            &ParserError::InvalidRuleHead =>
                "invalid_head_of_rule",
            &ParserError::InvalidUseModuleDecl =>
                "invalid_use_module_declaration",
            &ParserError::InvalidSingleQuotedCharacter =>
                "invalid_single_quoted_character",
            &ParserError::IO(_) =>
                "input_output_error",
            &ParserError::MissingQuote =>
                "missing_quote",
            &ParserError::NonPrologChar =>
                "non_prolog_character",
            &ParserError::ParseBigInt =>
                "cannot_parse_big_int",
            &ParserError::ParseFloat =>
                "cannot_parse_float",
            &ParserError::Utf8Error =>
                "utf8_conversion_error",
            &ParserError::CannotParseCyclicTerm =>
                "cannot_parse_cyclic_term"
        }
    }
}

impl From<ArithmeticError> for ParserError {
    fn from(err: ArithmeticError) -> ParserError {
        ParserError::Arithmetic(err)
    }
}

impl From<IOError> for ParserError {
    fn from(err: IOError) -> ParserError {
        ParserError::IO(err)
    }
}

impl From<Utf8Error> for ParserError {
    fn from(_: Utf8Error) -> ParserError {
        ParserError::Utf8Error
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum Fixity {
    In, Post, Pre
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SharedOpDesc(Rc<Cell<(usize, Specifier)>>);

impl SharedOpDesc {
    #[inline]
    pub fn new(priority: usize, spec: Specifier) -> Self {
        SharedOpDesc(Rc::new(Cell::new((priority, spec))))
    }

    #[inline]
    pub fn ptr_eq(lop_desc: &SharedOpDesc, rop_desc: &SharedOpDesc) -> bool {
        Rc::ptr_eq(&lop_desc.0, &rop_desc.0)
    }

    #[inline]
    pub fn arity(&self) -> usize {
        if self.get().1 & (XFX | XFY | YFX) == 0 {
            1
        } else {
            2
        }
    }

    #[inline]
    pub fn get(&self) -> (usize, Specifier) {
        self.0.get()
    }

    #[inline]
    pub fn set(&self, prec: usize, spec: Specifier) {
        self.0.set((prec, spec));
    }

    #[inline]
    pub fn prec(&self) -> usize {
        self.0.get().0
    }

    #[inline]
    pub fn assoc(&self) -> Specifier {
        self.0.get().1
    }
}

impl Hash for SharedOpDesc {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.get().hash(state);
    }
}

#[derive(Clone, Hash)]
pub enum Constant {
    Atom(ClauseName, Option<SharedOpDesc>),
    CharCode(u8),
    Char(char),
    Integer(Integer),
    Rational(Rational),
    Float(OrderedFloat<f64>),
    String(StringList),
    Usize(usize),
    EmptyList
}

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Constant::Atom(ref atom, _) =>
                if atom.as_str().chars().any(|c| "`.$'\" ".contains(c)) {
                    write!(f, "'{}'", atom.as_str())
                } else {
                    write!(f, "{}", atom.as_str())
                },
            &Constant::Char(c) =>
                write!(f, "'{}'", c as u8),
            &Constant::CharCode(c) =>
                write!(f, "{}", c),
            &Constant::EmptyList =>
                write!(f, "[]"),
            &Constant::Integer(ref n) =>
                write!(f, "{}", n),
            &Constant::Rational(ref n) =>
                write!(f, "{}", n),
            &Constant::Float(ref n) =>
                write!(f, "{}", n),
            &Constant::String(ref s) =>
                write!(f, "\"{}\"", s.borrow()),
            &Constant::Usize(integer) =>
                write!(f, "u{}", integer)
        }
    }
}

impl PartialEq for Constant {
    fn eq(&self, other: &Constant) -> bool {
        match (self, other) {
            (&Constant::Atom(ref atom, _), &Constant::Char(c))
          | (&Constant::Char(c), &Constant::Atom(ref atom, _)) => {
              let s = atom.as_str();
              c.len_utf8() == s.len() && Some(c) == s.chars().next()
            },
            (&Constant::Atom(ref a1, _), &Constant::Atom(ref a2, _)) =>
                a1.as_str() == a2.as_str(),
            (&Constant::Char(c1), &Constant::Char(c2)) =>
                c1 == c2,
            (&Constant::CharCode(c1), &Constant::CharCode(c2)) =>
                c1 == c2,
            (&Constant::CharCode(c1), &Constant::Integer(ref c2))
          | (&Constant::Integer(ref c2), &Constant::CharCode(c1)) =>
              match c2.to_u8() {
                  Some(c2) => c1 == c2,
                  None => false
              },
            (&Constant::Integer(ref n1), &Constant::Integer(ref n2)) =>
                n1 == n2,
            (&Constant::Rational(ref n1), &Constant::Rational(ref n2)) =>
                n1 == n2,
            (&Constant::Float(ref n1), &Constant::Float(ref n2)) =>
                n1 == n2,
            (&Constant::String(ref s1), &Constant::String(ref s2)) =>
                s1 == s2,
            (&Constant::EmptyList, &Constant::EmptyList) =>
                true,
            (&Constant::Usize(u1), &Constant::Usize(u2)) =>
                u1 == u2,
            _ => false
        }
    }
}

impl Eq for Constant {}

impl Constant {
    pub fn to_integer(self) -> Option<Integer> {
        match self {
            Constant::Integer(b) => Some(b),
            _ => None
        }
    }
    
    pub fn to_atom(self) -> Option<ClauseName> {
        match self {
            Constant::Atom(a, _) => Some(a.defrock_brackets()),
            _ => None
        }
    }
}

#[derive(Clone)]
pub enum ClauseName {
    BuiltIn(&'static str),
    User(TabledRc<Atom>)
}

impl fmt::Display for ClauseName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Hash for ClauseName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (*self.as_str()).hash(state)
    }
}

impl PartialEq for ClauseName {
    fn eq(&self, other: &ClauseName) -> bool {
        *self.as_str() == *other.as_str()
    }
}

impl Eq for ClauseName {}

impl Ord for ClauseName {
    fn cmp(&self, other: &ClauseName) -> Ordering {
        (*self.as_str()).cmp(other.as_str())
    }
}

impl PartialOrd for ClauseName {
    fn partial_cmp(&self, other: &ClauseName) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> From<&'a TabledRc<Atom>> for ClauseName {
    fn from(name: &'a TabledRc<Atom>) -> ClauseName {
        ClauseName::User(name.clone())
    }
}

impl ClauseName {
    #[inline]
    pub fn owning_module(&self) -> Self {
        match self {
            &ClauseName::User(ref name) => {
                let module = name.owning_module();
                 ClauseName::User(TabledRc { atom: module.clone(),
                                             table: TabledData::new(module) })
            },
            _ => clause_name!("user")
        }
    }

    #[inline]
    pub fn to_rc(&self) -> Rc<String> {
        match self {
            &ClauseName::BuiltIn(s) => Rc::new(s.to_string()),
            &ClauseName::User(ref rc) => rc.inner()
        }
    }

    #[inline]
    pub fn with_table(self, atom_tbl: TabledData<Atom>) -> Self {
        match self {
            ClauseName::BuiltIn(_) => self,
            ClauseName::User(mut name) => {
                name.table = atom_tbl;
                ClauseName::User(name)
            }
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            &ClauseName::BuiltIn(s) => s,
            &ClauseName::User(ref name) => name.as_ref()
        }
    }

    pub fn defrock_brackets(self) -> Self {
        fn defrock_brackets(s: &str) -> &str {
            if s.starts_with('(') && s.ends_with(')') {
                &s[1 .. s.len() - 1]
            } else {
                s
            }
        }

        match self {
            ClauseName::BuiltIn(s) =>
                ClauseName::BuiltIn(defrock_brackets(s)),
            ClauseName::User(s) =>
                ClauseName::User(tabled_rc!(defrock_brackets(s.as_str()).to_owned(), s.table))
        }
    }
}

impl AsRef<str> for ClauseName {
    #[inline]
    fn as_ref(self: &Self) -> &str {
        self.as_str()
    }
}

#[derive(PartialEq, Eq, Clone)]
pub enum Term {
    AnonVar,
    Clause(Cell<RegType>, ClauseName, Vec<Box<Term>>, Option<SharedOpDesc>),
    Cons(Cell<RegType>, Box<Term>, Box<Term>),
    Constant(Cell<RegType>, Constant),
    Var(Cell<VarReg>, Rc<Var>)
}

impl Term {
    pub fn shared_op_desc(&self) -> Option<SharedOpDesc> {
        match self {
            &Term::Clause(_, _, _, ref spec) => spec.clone(),
            &Term::Constant(_, Constant::Atom(_, ref spec)) => spec.clone(),
            _ => None
        }
    }

    pub fn to_constant(self) -> Option<Constant> {
        match self {
            Term::Constant(_, c) => Some(c),
            _ => None
        }
    }

    pub fn first_arg(&self) -> Option<&Term> {
        match self {
            &Term::Clause(_, _, ref terms, _) =>
                terms.first().map(|bt| bt.as_ref()),
            _ => None
        }
    }

    pub fn name(&self) -> Option<ClauseName> {
        match self {
            &Term::Constant(_, Constant::Atom(ref atom, _))
          | &Term::Clause(_, ref atom, ..) => Some(atom.clone()),
            _ => None
        }
    }

    pub fn arity(&self) -> usize {
        match self {
            &Term::Clause(_, _, ref child_terms, ..) => child_terms.len(),
            _ => 0
        }
    }
}

#[derive(Clone, Copy)]
pub struct CompositeOp<'a, 'b> {
    pub op_dir: &'a OpDir,
    pub static_op_dir: Option<&'b OpDir>
}

#[macro_export]
macro_rules! composite_op {
    ($include_machine_p:expr, $op_dir:expr, $machine_op_dir:expr) => (
        CompositeOp { op_dir: $op_dir,
                      static_op_dir: if !$include_machine_p {
                          Some($machine_op_dir)
                      } else {
                          None
                      }}
    );
    ($op_dir:expr) => (
        CompositeOp { op_dir: $op_dir, static_op_dir: None }
    )
}

impl<'a, 'b> CompositeOp<'a, 'b>
{
    #[inline]
    pub(crate)
    fn get(&self, name: ClauseName, fixity: Fixity) -> Option<OpDirValue>
    {
        self.op_dir.get(&(name.clone(), fixity))
            .or_else(move || self.static_op_dir.and_then(|op_dir| op_dir.get(&(name, fixity))))
            .cloned()
    }
}

pub type ParsingStream<R> = PutBackN<Bytes<R>>;

#[inline]
pub fn parsing_stream<R: Read>(src: R) -> ParsingStream<R> {
    put_back_n(src.bytes())
}
