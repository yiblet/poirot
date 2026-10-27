/// Structural rejection of persisted compiled programs.
#[derive(Debug, thiserror::Error)]
pub enum PlanSnapshotError {
    /// Allocation failed while checking which call owns each contextual reducer.
    #[error("could not reserve contextual reducer validation state")]
    AggregationReservation(#[source] std::collections::TryReserveError),
    /// Call source UTF-8.
    #[error("call source UTF-8")]
    CallSourceUTF8,
    /// Call source length.
    #[error("call source length")]
    CallSourceLength,
    /// Call source offset.
    #[error("call source offset")]
    CallSourceOffset,
    /// Call source overflow.
    #[error("call source overflow")]
    CallSourceOverflow,
    /// Call source text.
    #[error("call source text")]
    CallSourceText,
    /// Compiled program index.
    #[error("compiled program index")]
    CompiledProgramIndex,
    /// Compiled registry identity differs from overlay policy.
    #[error("compiled registry identity differs from overlay policy")]
    CompiledRegistryIdentityDiffersFromOverlayPolicy,
    /// Compiler cache policy index.
    #[error("compiler cache policy index")]
    CompilerCachePolicyIndex,
    /// Compiler snapshot count limit.
    #[error("compiler snapshot count limit")]
    CompilerSnapshotCountLimit,
    /// Compiler typing boundaries.
    #[error("compiler typing boundaries")]
    CompilerTypingBoundaries,
    /// Duplicate compiled source.
    #[error("duplicate compiled source")]
    DuplicateCompiledSource,
    /// Duplicate formula target.
    #[error("duplicate formula target")]
    DuplicateFormulaTarget,
    /// Registry mismatch in compiler cache.
    #[error("registry mismatch in compiler cache")]
    RegistryMismatchInCompilerCache,
    /// Snapshot UTF8 operand.
    #[error("snapshot UTF8 operand")]
    SnapshotUTF8Operand,
    /// Snapshot aggregation argument role.
    #[error("snapshot aggregation argument role")]
    SnapshotAggregationArgumentRole,
    /// Snapshot aggregation count.
    #[error("snapshot aggregation count")]
    SnapshotAggregationCount,
    /// Snapshot aggregation plan.
    #[error("snapshot aggregation plan")]
    SnapshotAggregationPlan,
    /// Snapshot argument chain.
    #[error("snapshot argument chain")]
    SnapshotArgumentChain,
    /// Snapshot argument count.
    #[error("snapshot argument count")]
    SnapshotArgumentCount,
    /// Snapshot argument descriptor.
    #[error("snapshot argument descriptor")]
    SnapshotArgumentDescriptor,
    /// Snapshot backwards invocation.
    #[error("snapshot backwards invocation")]
    SnapshotBackwardsInvocation,
    /// Snapshot backwards jump.
    #[error("snapshot backwards jump")]
    SnapshotBackwardsJump,
    /// Snapshot binary.
    #[error("snapshot binary")]
    SnapshotBinary,
    /// Snapshot builtin unavailable.
    #[error("snapshot builtin unavailable")]
    SnapshotBuiltinUnavailable,
    /// Snapshot call index/arity.
    #[error("snapshot call index/arity")]
    SnapshotCallIndexArity,
    /// Snapshot conditional binding.
    #[error("snapshot conditional binding")]
    SnapshotConditionalBinding,
    /// Snapshot defined-name identity.
    #[error("snapshot defined-name identity")]
    SnapshotDefinedNameIdentity,
    /// Snapshot does not support extensions.
    #[error("snapshot does not support extensions")]
    SnapshotDoesNotSupportExtensions,
    /// Snapshot does not support preparation failures.
    #[error("snapshot does not support preparation failures")]
    SnapshotDoesNotSupportPreparationFailures,
    /// Snapshot empty template.
    #[error("snapshot empty template")]
    SnapshotEmptyTemplate,
    /// Snapshot formula identity.
    #[error("snapshot formula identity")]
    SnapshotFormulaIdentity,
    /// Snapshot instruction flags.
    #[error("snapshot instruction flags")]
    SnapshotInstructionFlags,
    /// Snapshot invoke continuation.
    #[error("snapshot invoke continuation")]
    SnapshotInvokeContinuation,
    /// Snapshot invoke jump.
    #[error("snapshot invoke jump")]
    SnapshotInvokeJump,
    /// Snapshot jump label.
    #[error("snapshot jump label")]
    SnapshotJumpLabel,
    /// Snapshot label table.
    #[error("snapshot label table")]
    SnapshotLabelTable,
    /// Snapshot name source ID.
    #[error("snapshot name source ID")]
    SnapshotNameSourceID,
    /// Snapshot operand bounds.
    #[error("snapshot operand bounds")]
    SnapshotOperandBounds,
    /// Snapshot operand overflow.
    #[error("snapshot operand overflow")]
    SnapshotOperandOverflow,
    /// Snapshot program ID.
    #[error("snapshot program ID")]
    SnapshotProgramID,
    /// Snapshot qualified name UTF8.
    #[error("snapshot qualified name UTF8")]
    SnapshotQualifiedNameUTF8,
    /// Snapshot reference UTF8.
    #[error("snapshot reference UTF8")]
    SnapshotReferenceUTF8,
    /// Snapshot reference boolean.
    #[error("snapshot reference boolean")]
    SnapshotReferenceBoolean,
    /// Snapshot reference bounds.
    #[error("snapshot reference bounds")]
    SnapshotReferenceBounds,
    /// Snapshot reference coordinate.
    #[error("snapshot reference coordinate")]
    SnapshotReferenceCoordinate,
    /// Snapshot reference count/bytes.
    #[error("snapshot reference count/bytes")]
    SnapshotReferenceCountBytes,
    /// Snapshot reference index.
    #[error("snapshot reference index")]
    SnapshotReferenceIndex,
    /// Snapshot reference overflow.
    #[error("snapshot reference overflow")]
    SnapshotReferenceOverflow,
    /// Snapshot reference slot.
    #[error("snapshot reference slot")]
    SnapshotReferenceSlot,
    /// Snapshot reference trailing bytes.
    #[error("snapshot reference trailing bytes")]
    SnapshotReferenceTrailingBytes,
    /// Snapshot requires default policy/no extensions.
    #[error("snapshot requires default policy/no extensions")]
    SnapshotRequiresDefaultPolicyNoExtensions,
    /// Snapshot source ID.
    #[error("snapshot source ID")]
    SnapshotSourceID,
    /// Snapshot source topology count.
    #[error("snapshot source topology count")]
    SnapshotSourceTopologyCount,
    /// Snapshot truncated reference.
    #[error("snapshot truncated reference")]
    SnapshotTruncatedReference,
    /// Snapshot truncated string.
    #[error("snapshot truncated string")]
    SnapshotTruncatedString,
    /// Snapshot typing boundaries.
    #[error("snapshot typing boundaries")]
    SnapshotTypingBoundaries,
    /// Snapshot unary.
    #[error("snapshot unary")]
    SnapshotUnary,
    /// Unknown builtin ID.
    #[error("unknown builtin ID")]
    UnknownBuiltinID,
    /// Unsupported compiled snapshot version.
    #[error("unsupported compiled snapshot version")]
    UnsupportedCompiledSnapshotVersion,
    /// Persisted coordinate lies outside workbook bounds.
    #[error("persisted coordinate rejected")]
    Coordinate(#[from] poirot_io::ModelError),
}
