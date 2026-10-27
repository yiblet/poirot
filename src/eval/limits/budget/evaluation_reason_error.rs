/// Typed operation or capability rejection explaining an evaluation failure.
#[derive(Clone, Debug, thiserror::Error)]
pub enum EvaluationReasonError {
    /// CELL omitted its reference and requires a selected UI cell.
    #[error("CELL omitted reference requires selected UI cell")]
    CellSelectedUiReference,
    /// BAHTTEXT supports magnitudes below 2^53 baht; larger Excel spellings are unverified.
    #[error("BAHTTEXT supports magnitudes below 2^53 baht; larger Excel spellings are unverified")]
    BahtMagnitudeOutsideVerifiedDomain,
    /// CELL encountered a format string outside its recognized format whitelist.
    #[error("CELL custom number-format classification")]
    CellCustomNumberFormat,
    /// CELL requires unmodeled file, protection or display metadata.
    #[error("CELL requires unmodeled file, protection or display metadata")]
    CellUnmodeledMetadata,
    /// PIVOTBY cannot output field headers; use field_headers 0 or 1.
    #[error("PIVOTBY field-header publication is not supported; use field_headers 0 or 1")]
    PivotFieldHeaderPublication,
    /// PIVOTBY currently supports one value column and one built-in reducer.
    #[error("PIVOTBY currently supports one value column and one built-in reducer")]
    PivotMultipleValuesOrReducers,
    /// PIVOTBY requires a supported built-in reduction handle; callable values are not implemented.
    #[error(
        "PIVOTBY requires a supported built-in reduction handle; callable values are not implemented"
    )]
    PivotCallableReducer,
    /// The selected byte-text language requires unverified DBCS semantics.
    #[error("DBCS byte text is outside the verified non-DBCS contract")]
    DbcsByteText,
    /// The bounded XML backend rejected the requested operation.
    #[error("XML selection rejected")]
    Xml(
        #[source]
        std::sync::Arc<
            poirot_functions::text::xml::XmlQueryError<super::evaluation_error::EvaluationError>,
        >,
    ),

    /// Bessel computation exceeds the bounded order/series envelope.
    #[error("Bessel computation exceeds the bounded order/series envelope")]
    BesselComputationExceedsTheBoundedOrderSeriesEnvelope,
    /// ETS preparation exceeds its admitted allocation or work envelope.
    #[error("ETS preparation exceeds its admitted allocation or work envelope")]
    EtsPreparationExceedsItsAdmittedAllocationOrWorkEnvelope,
    /// Excel CODE requires an explicit non-ASCII platform code page.
    #[error("Excel CODE requires an explicit non-ASCII platform code page")]
    ExcelCodeRequiresAnExplicitNonAsciiPlatformCodePage,
    /// PCRE2 capture geometry did not satisfy the checked adapter state.
    #[error("PCRE2 capture geometry did not satisfy the checked adapter state")]
    Pcre2CaptureGeometryDidNotSatisfyTheCheckedAdapterState,
    /// RANDARRAY integer bounds exceed the exactly representable integer envelope.
    #[error("RANDARRAY integer bounds exceed the exactly representable integer envelope")]
    RandarrayIntegerBoundsExceedTheExactlyRepresentableIntegerEnvelope,
    /// TIMEVALUE dummy prefix length overflow.
    #[error("TIMEVALUE dummy prefix length overflow")]
    TimevalueDummyPrefixLengthOverflow,
    /// Unicode scalar consumer cannot project isolated UTF16 surrogates.
    #[error("Unicode scalar consumer cannot project isolated UTF16 surrogates")]
    UnicodeScalarConsumerCannotProjectIsolatedUtf16Surrogates,
    /// VM argument stack allocation failed.
    #[error("VM argument stack allocation failed")]
    VmArgumentStackAllocationFailed,
    /// Accrued interest exceeds its quasi-coupon envelope.
    #[error("accrued interest exceeds its quasi-coupon envelope")]
    AccruedInterestExceedsItsQuasiCouponEnvelope,
    /// Admitted cell journal allocation failed.
    #[error("admitted cell journal allocation failed")]
    AdmittedCellJournalAllocationFailed,
    /// Aggregate member allocation failed.
    #[error("aggregate member allocation failed")]
    AggregateMemberAllocationFailed,
    /// Aggregate member count overflow.
    #[error("aggregate member count overflow")]
    AggregateMemberCountOverflow,
    /// Algorithm resource guard refused execution; native counts are unavailable.
    #[error("algorithm resource guard refused execution; native counts are unavailable")]
    AlgorithmResourceGuardRefusedExecutionNativeCountsAreUnavailable,
    /// Allocating a function result or scratch buffer.
    #[error("allocating a function result or scratch buffer")]
    AllocatingAFunctionResultOrScratchBuffer,
    /// Array operator requires one bounded rectangular operand.
    #[error("array operator requires one bounded rectangular operand")]
    ArrayOperatorRequiresOneBoundedRectangularOperand,
    /// Array operator requires single-sheet rectangular operands.
    #[error("array operator requires single-sheet rectangular operands")]
    ArrayOperatorRequiresSingleSheetRectangularOperands,
    /// Array result reads its own unpublished footprint.
    #[error("array result reads its own unpublished footprint")]
    ArrayResultReadsItsOwnUnpublishedFootprint,
    /// Array result reads its own unpublished sparse footprint.
    #[error("array result reads its own unpublished sparse footprint")]
    ArrayResultReadsItsOwnUnpublishedSparseFootprint,
    /// Array shape overflow.
    #[error("array shape overflow")]
    ArrayShapeOverflow,
    /// Bond yield exceeds its numerical envelope.
    #[error("bond yield exceeds its numerical envelope")]
    BondYieldExceedsItsNumericalEnvelope,
    /// Both sheet-span endpoints unresolved; native classification unavailable.
    #[error("both sheet-span endpoints unresolved; native classification unavailable")]
    BothSheetSpanEndpointsUnresolvedNativeClassificationUnavailable,
    /// Bound extension implementation is absent or changed; rebind against the current registry.
    #[error(
        "bound extension implementation is absent or changed; rebind against the current registry"
    )]
    BoundExtensionImplementationIsAbsentOrChangedRebindAgainstTheCurrentRegistry,
    /// Bounded ETS optimizer did not converge.
    #[error("bounded ETS optimizer did not converge")]
    BoundedEtsOptimizerDidNotConverge,
    /// Bounded integer entropy rejection exhausted.
    #[error("bounded integer entropy rejection exhausted")]
    BoundedIntegerEntropyRejectionExhausted,
    /// Caller-trusted saved spill has an edited child.
    #[error("caller-trusted saved spill has an edited child")]
    CallerTrustedSavedSpillHasAnEditedChild,
    /// Caller-trusted saved spill is incomplete or lacks declared geometry.
    #[error("caller-trusted saved spill is incomplete or lacks declared geometry")]
    CallerTrustedSavedSpillIsIncompleteOrLacksDeclaredGeometry,
    /// Cell budget frame allocation failed.
    #[error("cell budget frame allocation failed")]
    CellBudgetFrameAllocationFailed,
    /// Cell execution bookkeeping allocation failed.
    #[error("cell execution bookkeeping allocation failed")]
    CellExecutionBookkeepingAllocationFailed,
    /// Cell memo allocation failed.
    #[error("cell memo allocation failed")]
    CellMemoAllocationFailed,
    /// Circular unpublished producer dependency.
    #[error("circular unpublished producer dependency")]
    CircularUnpublishedProducerDependency,
    /// Collection length does not fit this platform.
    #[error("collection length does not fit this platform")]
    CollectionLengthDoesNotFitThisPlatform,
    /// Comparison work size overflow.
    #[error("comparison work size overflow")]
    ComparisonWorkSizeOverflow,
    /// Complex functions have no verified omitted-argument coercion.
    #[error("complex functions have no verified omitted-argument coercion")]
    ComplexFunctionsHaveNoVerifiedOmittedArgumentCoercion,
    /// Computed array scalar coordinates are not established by its adapter.
    #[error("computed array scalar coordinates are not established by its adapter")]
    ComputedArrayScalarCoordinatesAreNotEstablishedByItsAdapter,
    /// Criteria domain error code.
    #[error("criteria domain error code")]
    CriteriaDomainErrorCode,
    /// Currency localization is not declared by the captured run configuration.
    #[error("currency localization is not declared by the captured run configuration")]
    CurrencyLocalizationIsNotDeclaredByTheCapturedRunConfiguration,
    /// Database formula criteria.
    #[error("database formula criteria")]
    DatabaseFormulaCriteria,
    /// Database scalar projection cannot preserve isolated UTF16 surrogates.
    #[error("database scalar projection cannot preserve isolated UTF16 surrogates")]
    DatabaseScalarProjectionCannotPreserveIsolatedUtf16Surrogates,
    /// Dense consumer policy for zero/negative Java source dimensions is not verified.
    #[error("dense consumer policy for zero/negative Java source dimensions is not verified")]
    DenseConsumerPolicyForZeroNegativeJavaSourceDimensionsIsNotVerified,
    /// Distribution computation exceeds its bounded work envelope.
    #[error("distribution computation exceeds its bounded work envelope")]
    DistributionComputationExceedsItsBoundedWorkEnvelope,
    /// Dynamic array publication requires a source formula cell; ad hoc probes are unsupported.
    #[error(
        "dynamic array publication requires a source formula cell; ad hoc probes are unsupported"
    )]
    DynamicArrayPublicationRequiresASourceFormulaCellAdHocProbesAreUnsupported,
    /// Dynamic execution encountered a circular unpublished footprint read.
    #[error("dynamic execution encountered a circular unpublished footprint read")]
    DynamicExecutionEncounteredACircularUnpublishedFootprintRead,
    /// Dynamic spill ownership is incompatible with an external scalar cell cache.
    #[error("dynamic spill ownership is incompatible with an external scalar cell cache")]
    DynamicSpillOwnershipIsIncompatibleWithAnExternalScalarCellCache,
    /// Evaluation reached the engine's 128-frame nesting ceiling.
    #[error("evaluation reached the engine's 128-frame nesting ceiling")]
    EvaluationReachedTheEngineS128FrameNestingCeiling,
    /// Execution evidence retention capacity exhausted.
    #[error("execution evidence retention capacity exhausted")]
    ExecutionEvidenceRetentionCapacityExhausted,
    /// Execution operation identity exhausted.
    #[error("execution operation identity exhausted")]
    ExecutionOperationIdentityExhausted,
    /// Extension registry identity changed; rebind the formula plan.
    #[error("extension registry identity changed; rebind the formula plan")]
    ExtensionRegistryIdentityChangedRebindTheFormulaPlan,
    /// Financial computation exceeds its bounded work envelope.
    #[error("financial computation exceeds its bounded work envelope")]
    FinancialComputationExceedsItsBoundedWorkEnvelope,
    /// Function size or work estimate overflow.
    #[error("function size or work estimate overflow")]
    FunctionSizeOrWorkEstimateOverflow,
    /// Grouping output allocation failed.
    #[error("grouping output allocation failed")]
    GroupingOutputAllocationFailed,
    /// Irregular financial calculation exceeds its work envelope.
    #[error("irregular financial calculation exceeds its work envelope")]
    IrregularFinancialCalculationExceedsItsWorkEnvelope,
    /// Legacy spill owner allocation failed.
    #[error("legacy spill owner allocation failed")]
    LegacySpillOwnerAllocationFailed,
    /// Legacy spill owner ordering allocation failed.
    #[error("legacy spill owner ordering allocation failed")]
    LegacySpillOwnerOrderingAllocationFailed,
    /// Legacy spill owner shared allocation overflow.
    #[error("legacy spill owner shared allocation overflow")]
    LegacySpillOwnerSharedAllocationOverflow,
    /// Lookup reference output requires unavailable external/3D identity.
    #[error("lookup reference output requires unavailable external/3D identity")]
    LookupReferenceOutputRequiresUnavailableExternal3DIdentity,
    /// Matrix materialization of a multi-sheet reference is not verified.
    #[error("matrix materialization of a multi-sheet reference is not verified")]
    MatrixMaterializationOfAMultiSheetReferenceIsNotVerified,
    /// Numeric date default year outside supported calendar.
    #[error("numeric date default year outside supported calendar")]
    NumericDateDefaultYearOutsideSupportedCalendar,
    /// Numeric date default year requires a supported frozen clock.
    #[error("numeric date default year requires a supported frozen clock")]
    NumericDateDefaultYearRequiresASupportedFrozenClock,
    /// Numeric date dependency size overflow.
    #[error("numeric date dependency size overflow")]
    NumericDateDependencySizeOverflow,
    /// Omitted SUBTOTAL selector.
    #[error("omitted SUBTOTAL selector")]
    OmittedSubtotalSelector,
    /// Omitted conditional control.
    #[error("omitted conditional control")]
    OmittedConditionalControl,
    /// Pinned HSSF missing last sheet endpoint throws java.lang.IllegalArgumentException during formula authoring.
    #[error(
        "pinned HSSF missing last sheet endpoint throws java.lang.IllegalArgumentException during formula authoring"
    )]
    PinnedHssfMissingLastSheetEndpointThrowsJavaLangIllegalArgumentExceptionDuringFormulaAuthoring,
    /// Pinned HSSF reversed sheet endpoints throw java.lang.NegativeArraySizeException during evaluation.
    #[error(
        "pinned HSSF reversed sheet endpoints throw java.lang.NegativeArraySizeException during evaluation"
    )]
    PinnedHssfReversedSheetEndpointsThrowJavaLangNegativeArraySizeExceptionDuringEvaluation,
    /// Pinned special-number aggregate algorithm is not implemented for this handler.
    #[error("pinned special-number aggregate algorithm is not implemented for this handler")]
    PinnedSpecialNumberAggregateAlgorithmIsNotImplementedForThisHandler,
    /// Producer self-read observation allocation failed.
    #[error("producer self-read observation allocation failed")]
    ProducerSelfReadObservationAllocationFailed,
    /// Provider contract does not allow array arguments.
    #[error("provider contract does not allow array arguments")]
    ProviderContractDoesNotAllowArrayArguments,
    /// Provider contract does not allow array output.
    #[error("provider contract does not allow array output")]
    ProviderContractDoesNotAllowArrayOutput,
    /// Provider contract does not allow evaluation context.
    #[error("provider contract does not allow evaluation context")]
    ProviderContractDoesNotAllowEvaluationContext,
    /// Provider contract does not allow evaluation position.
    #[error("provider contract does not allow evaluation position")]
    ProviderContractDoesNotAllowEvaluationPosition,
    /// Provider contract does not allow reference arguments.
    #[error("provider contract does not allow reference arguments")]
    ProviderContractDoesNotAllowReferenceArguments,
    /// Random integer interval exceeds the calculation stream's exact domain.
    #[error("random integer interval exceeds the calculation stream's exact domain")]
    RandomIntegerIntervalExceedsTheCalculationStreamSExactDomain,
    /// Rectangular member allocation.
    #[error("rectangular member allocation")]
    RectangularMemberAllocation,
    /// Rectangular row allocation.
    #[error("rectangular row allocation")]
    RectangularRowAllocation,
    /// Sample statistics exceeds bounded numerical work.
    #[error("sample statistics exceeds bounded numerical work")]
    SampleStatisticsExceedsBoundedNumericalWork,
    /// Saved spill row allocation failed.
    #[error("saved spill row allocation failed")]
    SavedSpillRowAllocationFailed,
    /// Saved spill value allocation failed.
    #[error("saved spill value allocation failed")]
    SavedSpillValueAllocationFailed,
    /// Selected branch allocation failed.
    #[error("selected branch allocation failed")]
    SelectedBranchAllocationFailed,
    /// Selection allocation failed.
    #[error("selection allocation failed")]
    SelectionAllocationFailed,
    /// Selection expansion allocation.
    #[error("selection expansion allocation")]
    SelectionExpansionAllocation,
    /// Selection requires a rectangular expression.
    #[error("selection requires a rectangular expression")]
    SelectionRequiresARectangularExpression,
    /// Selection requires one rectangular condition.
    #[error("selection requires one rectangular condition")]
    SelectionRequiresOneRectangularCondition,
    /// Selection requires one rectangular selected branch.
    #[error("selection requires one rectangular selected branch")]
    SelectionRequiresOneRectangularSelectedBranch,
    /// Selection requires rectangular candidates.
    #[error("selection requires rectangular candidates")]
    SelectionRequiresRectangularCandidates,
    /// Selection requires rectangular selected results.
    #[error("selection requires rectangular selected results")]
    SelectionRequiresRectangularSelectedResults,
    /// Settled spill membership allocation failed.
    #[error("settled spill membership allocation failed")]
    SettledSpillMembershipAllocationFailed,
    /// Shape port cannot represent zero/negative Java source dimensions.
    #[error("shape port cannot represent zero/negative Java source dimensions")]
    ShapePortCannotRepresentZeroNegativeJavaSourceDimensions,
    /// Signed OFFSET requires a valid local single-sheet identity.
    #[error("signed OFFSET requires a valid local single-sheet identity")]
    SignedOffsetRequiresAValidLocalSingleSheetIdentity,
    /// Signed union cannot be coerced to scalar.
    #[error("signed union cannot be coerced to scalar")]
    SignedUnionCannotBeCoercedToScalar,
    /// Sparse coordinate allocation failed.
    #[error("sparse coordinate allocation failed")]
    SparseCoordinateAllocationFailed,
    /// Spill admission consumer allocation failed.
    #[error("spill admission consumer allocation failed")]
    SpillAdmissionConsumerAllocationFailed,
    /// Spill admission rollback allocation failed.
    #[error("spill admission rollback allocation failed")]
    SpillAdmissionRollbackAllocationFailed,
    /// Spill arbitration allocation failed.
    #[error("spill arbitration allocation failed")]
    SpillArbitrationAllocationFailed,
    /// Spill blocked-owner allocation failed.
    #[error("spill blocked-owner allocation failed")]
    SpillBlockedOwnerAllocationFailed,
    /// Spill candidate allocation failed.
    #[error("spill candidate allocation failed")]
    SpillCandidateAllocationFailed,
    /// Spill candidate append allocation failed.
    #[error("spill candidate append allocation failed")]
    SpillCandidateAppendAllocationFailed,
    /// Spill candidate row summary allocation failed.
    #[error("spill candidate row summary allocation failed")]
    SpillCandidateRowSummaryAllocationFailed,
    /// Spill consumer journal allocation failed.
    #[error("spill consumer journal allocation failed")]
    SpillConsumerJournalAllocationFailed,
    /// Spill discovery strip allocation failed.
    #[error("spill discovery strip allocation failed")]
    SpillDiscoveryStripAllocationFailed,
    /// Spill frontier allocation failed.
    #[error("spill frontier allocation failed")]
    SpillFrontierAllocationFailed,
    /// Spill frontier replacement allocation failed.
    #[error("spill frontier replacement allocation failed")]
    SpillFrontierReplacementAllocationFailed,
    /// Spill journal allocation failed.
    #[error("spill journal allocation failed")]
    SpillJournalAllocationFailed,
    /// Spill notification allocation failed.
    #[error("spill notification allocation failed")]
    SpillNotificationAllocationFailed,
    /// Spill ownership component does not settle.
    #[error("spill ownership component does not settle")]
    SpillOwnershipComponentDoesNotSettle,
    /// Spill role allocation failed.
    #[error("spill role allocation failed")]
    SpillRoleAllocationFailed,
    /// Spill rollback journal allocation failed.
    #[error("spill rollback journal allocation failed")]
    SpillRollbackJournalAllocationFailed,
    /// Spill settlement history allocation failed.
    #[error("spill settlement history allocation failed")]
    SpillSettlementHistoryAllocationFailed,
    /// Spill settlement would replay an actually consumed opaque or volatile provider.
    #[error("spill settlement would replay an actually consumed opaque or volatile provider")]
    SpillSettlementWouldReplayAnActuallyConsumedOpaqueOrVolatileProvider,
    /// This coordinate port has no verified multi-sheet selection policy.
    #[error("this coordinate port has no verified multi-sheet selection policy")]
    ThisCoordinatePortHasNoVerifiedMultiSheetSelectionPolicy,
    /// Failure reported by the identifier subsystem.
    #[error("identifier operation rejected")]
    Identifier(#[source] std::sync::Arc<crate::model::identifiers::IdentifierError>),
    /// Failure reported by the format subsystem.
    #[error("format operation rejected")]
    Format(#[source] std::sync::Arc<poirot_functions::format::FormatError>),
    /// Failure reported by the parse subsystem.
    #[error("parse operation rejected")]
    Parse(#[source] std::sync::Arc<poirot_parsing::formula::ParseError>),
    /// Failure reported by the lookup subsystem.
    #[error("lookup operation rejected")]
    Lookup(#[source] std::sync::Arc<poirot_functions::lookup::LookupError>),
    /// Failure reported by the textsemantics subsystem.
    #[error("textsemantics operation rejected")]
    TextSemantics(#[source] std::sync::Arc<poirot_functions::text_units::TextSemanticsError>),
    /// Compiler rejected resourcelimit.
    #[error("compiler rejected resourcelimit")]
    CompilationResourceLimit,
    /// Compiler rejected invalidarray.
    #[error("compiler rejected invalidarray")]
    CompilationInvalidArray,
    /// Compiler rejected invalidreference.
    #[error("compiler rejected invalidreference")]
    CompilationInvalidReference,
    /// Native regular expression engine rejection.
    #[error("regular expression engine rejected operation")]
    Regex(
        #[source]
        std::sync::Arc<
            poirot_functions::text::RegexError<super::evaluation_error::EvaluationError>,
        >,
    ),
    /// Registered function lacks an executable implementation.
    #[error("registered function {function} has no executable adapter")]
    MissingFunctionAdapter {
        /// Registered function spelling.
        function: String,
    },
    /// Matrix source behavior is not supported by the algorithm.
    #[error("matrix source behavior rejected")]
    Matrix(#[source] std::sync::Arc<poirot_functions::matrix::MatrixError>),
    /// Numeric-text representation is unsupported.
    #[error("numeric text representation rejected")]
    NumericText(#[source] std::sync::Arc<poirot_functions::coercion::NumericTextSourceError>),
    /// An external source cannot supply the requested cached or authored data.
    #[error("external source unavailable")]
    ExternalSourceUnavailable,
    /// Caller observer rejected an execution event.
    #[error("execution observer rejected event")]
    ObserverRejected,
    /// Numeric date-time coercion requires an unsupported policy or calendar value.
    #[error("date-time coercion rejected")]
    DateTime(#[source] std::sync::Arc<poirot_functions::coercion::DateTimeError>),
    /// The allocator rejected a storage reservation for a typed operation.
    #[error("storage reservation rejected during {operation}")]
    Reservation {
        /// Operation requiring storage.
        operation: Box<EvaluationReasonError>,
        /// Original allocator or representation failure.
        #[source]
        source: std::sync::Arc<std::collections::TryReserveError>,
    },
    /// The small-vector dependency rejected a parameter-storage reservation.
    #[error("small vector storage reservation rejected")]
    SmallVectorReservation {
        /// Native structured diagnostic; smallvec does not implement std::error::Error.
        native: std::sync::Arc<smallvec::CollectionAllocErr>,
    },
    /// The statistical source algorithm could not converge.
    #[error("statistical numerical search did not converge")]
    StatisticalConvergence(
        #[source] std::sync::Arc<poirot_functions::statistics::SourceConvergenceError>,
    ),
    /// Scalar projection was asked to select among separate reference areas.
    #[error("scalar projection requires a single reference area")]
    ScalarProjectionRequiresSingleReferenceArea,
    /// An omitted argument has no supported worksheet publication value.
    #[error("worksheet publication of a selected omitted argument is unsupported")]
    SelectedOmittedArgumentPublication,
    /// A selected omitted argument has no supported scalar projection.
    #[error("selection cannot project a selected omitted argument to a scalar")]
    SelectedOmittedArgumentScalarProjection,
}

impl EvaluationReasonError {
    /// Count domain text copied when retaining this reason; shared causes copy no text.
    pub(in crate::eval) fn retained_text_bytes(&self) -> usize {
        match self {
            Self::MissingFunctionAdapter { function } => function.len(),
            Self::Reservation { operation, .. } => operation.retained_text_bytes(),
            Self::SmallVectorReservation { .. } => 0,
            | Self::BahtMagnitudeOutsideVerifiedDomain
            | Self::CellCustomNumberFormat
            | Self::CellUnmodeledMetadata
            | Self::PivotFieldHeaderPublication
            | Self::PivotMultipleValuesOrReducers
            | Self::PivotCallableReducer
            | Self::CellSelectedUiReference
            | Self::DbcsByteText
            | Self::Xml(..)
            | Self::BesselComputationExceedsTheBoundedOrderSeriesEnvelope
            | Self::EtsPreparationExceedsItsAdmittedAllocationOrWorkEnvelope
            | Self::ExcelCodeRequiresAnExplicitNonAsciiPlatformCodePage
            | Self::Pcre2CaptureGeometryDidNotSatisfyTheCheckedAdapterState
            | Self::RandarrayIntegerBoundsExceedTheExactlyRepresentableIntegerEnvelope
            | Self::TimevalueDummyPrefixLengthOverflow
            | Self::UnicodeScalarConsumerCannotProjectIsolatedUtf16Surrogates
            | Self::VmArgumentStackAllocationFailed
            | Self::AccruedInterestExceedsItsQuasiCouponEnvelope
            | Self::AdmittedCellJournalAllocationFailed
            | Self::AggregateMemberAllocationFailed
            | Self::AggregateMemberCountOverflow
            | Self::AlgorithmResourceGuardRefusedExecutionNativeCountsAreUnavailable
            | Self::AllocatingAFunctionResultOrScratchBuffer
            | Self::ArrayOperatorRequiresOneBoundedRectangularOperand
            | Self::ArrayOperatorRequiresSingleSheetRectangularOperands
            | Self::ArrayResultReadsItsOwnUnpublishedFootprint
            | Self::ArrayResultReadsItsOwnUnpublishedSparseFootprint
            | Self::ArrayShapeOverflow
            | Self::BondYieldExceedsItsNumericalEnvelope
            | Self::BothSheetSpanEndpointsUnresolvedNativeClassificationUnavailable
            | Self::BoundExtensionImplementationIsAbsentOrChangedRebindAgainstTheCurrentRegistry
            | Self::BoundedEtsOptimizerDidNotConverge
            | Self::BoundedIntegerEntropyRejectionExhausted
            | Self::CallerTrustedSavedSpillHasAnEditedChild
            | Self::CallerTrustedSavedSpillIsIncompleteOrLacksDeclaredGeometry
            | Self::CellBudgetFrameAllocationFailed
            | Self::CellExecutionBookkeepingAllocationFailed
            | Self::CellMemoAllocationFailed
            | Self::CircularUnpublishedProducerDependency
            | Self::CollectionLengthDoesNotFitThisPlatform
            | Self::ComparisonWorkSizeOverflow
            | Self::ComplexFunctionsHaveNoVerifiedOmittedArgumentCoercion
            | Self::ComputedArrayScalarCoordinatesAreNotEstablishedByItsAdapter
            | Self::CriteriaDomainErrorCode
            | Self::CurrencyLocalizationIsNotDeclaredByTheCapturedRunConfiguration
            | Self::DatabaseFormulaCriteria
            | Self::DatabaseScalarProjectionCannotPreserveIsolatedUtf16Surrogates
            | Self::DenseConsumerPolicyForZeroNegativeJavaSourceDimensionsIsNotVerified
            | Self::DistributionComputationExceedsItsBoundedWorkEnvelope
            | Self::DynamicArrayPublicationRequiresASourceFormulaCellAdHocProbesAreUnsupported
            | Self::DynamicExecutionEncounteredACircularUnpublishedFootprintRead
            | Self::DynamicSpillOwnershipIsIncompatibleWithAnExternalScalarCellCache
            | Self::EvaluationReachedTheEngineS128FrameNestingCeiling
            | Self::ExecutionEvidenceRetentionCapacityExhausted
            | Self::ExecutionOperationIdentityExhausted
            | Self::ExtensionRegistryIdentityChangedRebindTheFormulaPlan
            | Self::FinancialComputationExceedsItsBoundedWorkEnvelope
            | Self::FunctionSizeOrWorkEstimateOverflow
            | Self::GroupingOutputAllocationFailed
            | Self::IrregularFinancialCalculationExceedsItsWorkEnvelope
            | Self::LegacySpillOwnerAllocationFailed
            | Self::LegacySpillOwnerOrderingAllocationFailed
            | Self::LegacySpillOwnerSharedAllocationOverflow
            | Self::LookupReferenceOutputRequiresUnavailableExternal3DIdentity
            | Self::MatrixMaterializationOfAMultiSheetReferenceIsNotVerified
            | Self::NumericDateDefaultYearOutsideSupportedCalendar
            | Self::NumericDateDefaultYearRequiresASupportedFrozenClock
            | Self::NumericDateDependencySizeOverflow
            | Self::OmittedSubtotalSelector
            | Self::OmittedConditionalControl
            | Self::PinnedHssfMissingLastSheetEndpointThrowsJavaLangIllegalArgumentExceptionDuringFormulaAuthoring
            | Self::PinnedHssfReversedSheetEndpointsThrowJavaLangNegativeArraySizeExceptionDuringEvaluation

            | Self::PinnedSpecialNumberAggregateAlgorithmIsNotImplementedForThisHandler
            | Self::ProducerSelfReadObservationAllocationFailed
            | Self::ProviderContractDoesNotAllowArrayArguments
            | Self::ProviderContractDoesNotAllowArrayOutput
            | Self::ProviderContractDoesNotAllowEvaluationContext
            | Self::ProviderContractDoesNotAllowEvaluationPosition
            | Self::ProviderContractDoesNotAllowReferenceArguments
            | Self::RandomIntegerIntervalExceedsTheCalculationStreamSExactDomain
            | Self::RectangularMemberAllocation
            | Self::RectangularRowAllocation
            | Self::SampleStatisticsExceedsBoundedNumericalWork
            | Self::SavedSpillRowAllocationFailed
            | Self::SavedSpillValueAllocationFailed
            | Self::SelectedBranchAllocationFailed
            | Self::SelectionAllocationFailed
            | Self::SelectionExpansionAllocation
            | Self::SelectionRequiresARectangularExpression
            | Self::SelectionRequiresOneRectangularCondition
            | Self::SelectionRequiresOneRectangularSelectedBranch
            | Self::SelectionRequiresRectangularCandidates
            | Self::SelectionRequiresRectangularSelectedResults
            | Self::SettledSpillMembershipAllocationFailed
            | Self::ShapePortCannotRepresentZeroNegativeJavaSourceDimensions
            | Self::SignedOffsetRequiresAValidLocalSingleSheetIdentity
            | Self::SignedUnionCannotBeCoercedToScalar
            | Self::SparseCoordinateAllocationFailed
            | Self::SpillAdmissionConsumerAllocationFailed
            | Self::SpillAdmissionRollbackAllocationFailed
            | Self::SpillArbitrationAllocationFailed
            | Self::SpillBlockedOwnerAllocationFailed
            | Self::SpillCandidateAllocationFailed
            | Self::SpillCandidateAppendAllocationFailed
            | Self::SpillCandidateRowSummaryAllocationFailed
            | Self::SpillConsumerJournalAllocationFailed
            | Self::SpillDiscoveryStripAllocationFailed
            | Self::SpillFrontierAllocationFailed
            | Self::SpillFrontierReplacementAllocationFailed
            | Self::SpillJournalAllocationFailed
            | Self::SpillNotificationAllocationFailed
            | Self::SpillOwnershipComponentDoesNotSettle
            | Self::SpillRoleAllocationFailed
            | Self::SpillRollbackJournalAllocationFailed
            | Self::SpillSettlementHistoryAllocationFailed
            | Self::SpillSettlementWouldReplayAnActuallyConsumedOpaqueOrVolatileProvider
            | Self::ThisCoordinatePortHasNoVerifiedMultiSheetSelectionPolicy

            | Self::DateTime(..)
            | Self::Identifier(..)
            | Self::Format(..)
            | Self::StatisticalConvergence(..)
            | Self::Parse(..)
            | Self::Lookup(..)
            | Self::TextSemantics(..)
            | Self::CompilationResourceLimit
            | Self::CompilationInvalidArray
            | Self::CompilationInvalidReference
            | Self::Regex(..)
            | Self::Matrix(..)
            | Self::NumericText(..)
            | Self::ScalarProjectionRequiresSingleReferenceArea
            | Self::SelectedOmittedArgumentPublication
            | Self::SelectedOmittedArgumentScalarProjection
            | Self::ExternalSourceUnavailable
            | Self::ObserverRejected => 0,
        }
    }

    /// Retain the allocator failure with the operation that requested storage.
    pub(in crate::eval) fn with_reservation_source(
        self,
        source: std::collections::TryReserveError,
    ) -> Self {
        Self::Reservation {
            operation: Box::new(self),
            source: std::sync::Arc::new(source),
        }
    }
}
