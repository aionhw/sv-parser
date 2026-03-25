//! SystemVerilog declarations (IEEE 1800-2017 §A.2)

use super::{Identifier, Span};
use super::expr::Expression;
use super::stmt::{Statement, VarDeclarator};
use super::types::*;

#[derive(Debug, Clone)]
pub enum ModuleItem {
    PortDeclaration(PortDeclaration),
    NetDeclaration(NetDeclaration),
    DataDeclaration(DataDeclaration),
    ParameterDeclaration(ParameterDeclaration),
    LocalparamDeclaration(ParameterDeclaration),
    TypedefDeclaration(TypedefDeclaration),
    AlwaysConstruct(AlwaysConstruct),
    InitialConstruct(InitialConstruct),
    FinalConstruct(FinalConstruct),
    ContinuousAssign(ContinuousAssign),
    ModuleInstantiation(ModuleInstantiation),
    GateInstantiation(GateInstantiation),
    GenerateRegion(GenerateRegion),
    /// Generate-if: condition + then-items, and a chain of (condition, items) for else-if/else
    GenerateIf(GenerateIf),
    GenvarDeclaration(GenvarDeclaration),
    FunctionDeclaration(FunctionDeclaration),
    TaskDeclaration(TaskDeclaration),
    ImportDeclaration(ImportDeclaration),
    ClassDeclaration(ClassDeclaration),
    AssertionItem(super::stmt::AssertionStatement),
    Null,
}

/// Verilog gate-level primitive instantiation (IEEE 1800-2017 §28)
/// e.g., `and and0 (out, in1, in2);`  `not not0 (out, in);`
#[derive(Debug, Clone)]
pub struct GateInstantiation {
    pub gate_type: GateType,
    pub instances: Vec<GateInstance>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct GateInstance {
    pub name: Option<Identifier>,
    /// First element is output, rest are inputs (for most gates).
    /// For buf/not: first is output, last is input.
    pub terminals: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GateType {
    And, Nand, Or, Nor, Xor, Xnor,
    Buf, Not,
    Bufif0, Bufif1, Notif0, Notif1,
}

#[derive(Debug, Clone)]
pub struct PortDeclaration {
    pub direction: PortDirection,
    pub net_type: Option<NetType>,
    pub data_type: DataType,
    pub declarators: Vec<VarDeclarator>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct NetDeclaration {
    pub net_type: NetType,
    pub strength: Option<String>,
    pub data_type: DataType,
    pub delay: Option<Expression>,
    pub declarators: Vec<NetDeclarator>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct NetDeclarator {
    pub name: Identifier,
    pub dimensions: Vec<UnpackedDimension>,
    pub init: Option<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct DataDeclaration {
    pub const_kw: bool,
    pub var_kw: bool,
    pub lifetime: Option<Lifetime>,
    pub data_type: DataType,
    pub declarators: Vec<VarDeclarator>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ParameterDeclaration {
    pub local: bool,
    pub kind: ParameterKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ParameterKind {
    Data { data_type: DataType, assignments: Vec<ParamAssignment> },
    Type { assignments: Vec<TypeParamAssignment> },
}

#[derive(Debug, Clone)]
pub struct ParamAssignment {
    pub name: Identifier,
    pub dimensions: Vec<UnpackedDimension>,
    pub init: Option<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeParamAssignment {
    pub name: Identifier,
    pub init: Option<DataType>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypedefDeclaration {
    pub data_type: DataType,
    pub name: Identifier,
    pub dimensions: Vec<UnpackedDimension>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlwaysKind { Always, AlwaysComb, AlwaysFf, AlwaysLatch }

#[derive(Debug, Clone)]
pub struct AlwaysConstruct {
    pub kind: AlwaysKind,
    pub stmt: Statement,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InitialConstruct {
    pub stmt: Statement,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FinalConstruct {
    pub stmt: Statement,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ContinuousAssign {
    pub strength: Option<String>,
    pub delay: Option<Expression>,
    pub assignments: Vec<(Expression, Expression)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ModuleInstantiation {
    pub module_name: Identifier,
    pub params: Option<Vec<ParamConnection>>,
    pub instances: Vec<HierarchicalInstance>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ParamConnection {
    Ordered(Option<Expression>),
    Named { name: Identifier, value: Option<Expression> },
}

#[derive(Debug, Clone)]
pub struct HierarchicalInstance {
    pub name: Identifier,
    pub dimensions: Vec<UnpackedDimension>,
    pub connections: Vec<PortConnection>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum PortConnection {
    Ordered(Option<Expression>),
    Named { name: Identifier, expr: Option<Expression> },
    Wildcard,
}

#[derive(Debug, Clone)]
pub struct GenerateRegion {
    pub items: Vec<ModuleItem>,
    pub span: Span,
}

/// A generate-if construct: if (cond) items [else if (cond) items]* [else items]
#[derive(Debug, Clone)]
pub struct GenerateIf {
    /// Chain of (condition, items). Last entry may have None condition for `else`.
    pub branches: Vec<(Option<super::expr::Expression>, Vec<ModuleItem>)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct GenvarDeclaration {
    pub names: Vec<Identifier>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub lifetime: Option<Lifetime>,
    pub return_type: DataType,
    pub name: Identifier,
    pub ports: Vec<FunctionPort>,
    pub items: Vec<Statement>,
    pub endlabel: Option<Identifier>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TaskDeclaration {
    pub lifetime: Option<Lifetime>,
    pub name: Identifier,
    pub ports: Vec<FunctionPort>,
    pub items: Vec<Statement>,
    pub endlabel: Option<Identifier>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionPort {
    pub direction: PortDirection,
    pub var_kw: bool,
    pub data_type: DataType,
    pub name: Identifier,
    pub dimensions: Vec<UnpackedDimension>,
    pub default: Option<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub items: Vec<ImportItem>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportItem {
    pub package: Identifier,
    pub item: Option<Identifier>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TimeunitsDeclaration {
    pub unit: Option<String>,
    pub precision: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassDeclaration {
    pub virtual_kw: bool,
    pub name: Identifier,
    pub params: Vec<ParameterDeclaration>,
    pub extends: Option<ClassExtends>,
    pub implements: Vec<Identifier>,
    pub items: Vec<ClassItem>,
    pub endlabel: Option<Identifier>,
    pub span: Span,
}

/// extends clause: `extends base_class [(args)]`
#[derive(Debug, Clone)]
pub struct ClassExtends {
    pub name: Identifier,
    pub args: Vec<super::expr::Expression>,
    pub span: Span,
}

/// Items that can appear inside a class body.
#[derive(Debug, Clone)]
pub enum ClassItem {
    /// Property: class member variable
    Property(ClassProperty),
    /// Method: function or task
    Method(ClassMethod),
    /// Constraint declaration
    Constraint(ClassConstraint),
    /// Typedef inside class
    Typedef(TypedefDeclaration),
    /// Parameter/localparam inside class
    Parameter(ParameterDeclaration),
    /// Class inside class (nested)
    Class(ClassDeclaration),
    /// Covergroup inside class
    Covergroup { name: Identifier, span: Span },
    /// Import statement
    Import(ImportDeclaration),
    /// Empty item (stray semicolons)
    Empty,
}

/// Class property (member variable).
#[derive(Debug, Clone)]
pub struct ClassProperty {
    pub qualifiers: Vec<ClassQualifier>,
    pub data_type: super::types::DataType,
    pub declarators: Vec<VarDeclarator>,
    pub span: Span,
}

/// Class method (function/task).
#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub qualifiers: Vec<ClassQualifier>,
    pub kind: ClassMethodKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ClassMethodKind {
    Function(FunctionDeclaration),
    Task(TaskDeclaration),
    /// Pure virtual prototype: `pure virtual function ...;`
    PureVirtual(FunctionDeclaration),
    /// extern method (body defined outside class)
    Extern(FunctionDeclaration),
}

/// Class constraint.
#[derive(Debug, Clone)]
pub struct ClassConstraint {
    pub is_static: bool,
    pub name: Identifier,
    /// Constraint body tokens (unparsed — constraint expressions are complex)
    pub span: Span,
}

/// Qualifiers for class properties and methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassQualifier {
    Static,
    Protected,
    Local,
    Rand,
    Randc,
    Virtual,
    Pure,
    Extern,
    Const,
}

#[derive(Debug, Clone)]
pub enum PackageItem {
    Parameter(ParameterDeclaration),
    Typedef(TypedefDeclaration),
    Function(FunctionDeclaration),
    Task(TaskDeclaration),
    Import(ImportDeclaration),
    Data(DataDeclaration),
    Class(ClassDeclaration),
}
