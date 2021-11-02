initSidebarItems({"constant":[["MILLISECS_PER_BLOCK","Since Subspace is probabilistic this is the average expected block time that we are targeting. Blocks will be produced at a minimum duration defined by `SLOT_DURATION`, but some slots will not be allocated to any farmer and hence no block will be produced. We expect to have this block time on average following the defined slot duration and the value of `c` configured for Subspace (where `1 - c` represents the probability of a slot being empty). This value is only used indirectly to define the unit constants below that are expressed in blocks. The rest of the code should use `SLOT_DURATION` instead (like the Timestamp pallet for calculating the minimum period)."],["SHANNON","The smallest unit of the token is called Shannon."],["SSC","One Subspace Credit has 18 decimal places."],["SUBSPACE_GENESIS_EPOCH_CONFIG","The Subspace epoch configuration at genesis."],["VERSION",""],["WASM_BINARY",""],["WASM_BINARY_BLOATY",""]],"enum":[["Call",""],["Event",""],["OriginCaller",""]],"fn":[["native_version","The version information used to identify this runtime when compiled natively."]],"mod":[["api",""],["opaque","Opaque types. These are used by the CLI to instantiate machinery that don’t need to know the specifics of the runtime. They can then be made to be agnostic over specific formats of data like extrinsics, allowing for them to continue syncing the network through upgrades to even the core data structures."]],"struct":[["BlockHashCount",""],["ConfirmationDepthK",""],["EonDuration",""],["EpochDuration",""],["EraDuration",""],["ExistentialDeposit",""],["ExpectedBlockTime",""],["GenesisConfig",""],["InitialSolutionRange",""],["MaxLocks",""],["MaxVestingSchedules",""],["MinVestedTransfer",""],["MinimumPeriod",""],["Origin",""],["PalletInfo","Provides an implementation of `PalletInfo` to provide information about the pallet setup in the runtime."],["PreGenesisObjectCount",""],["PreGenesisObjectSeed",""],["PreGenesisObjectSize",""],["RecordSize",""],["RecordedHistorySegmentSize",""],["ReportLongevity",""],["Runtime",""],["RuntimeApi",""],["RuntimeApiImpl","Implements all runtime apis for the client side."],["SS58Prefix",""],["SlotProbability",""],["SubspaceBlockLength","We allow for 3.75 MiB for `Normal` extrinsic with 5 MiB maximum block length."],["SubspaceBlockWeights","We allow for 2 seconds of compute with a 6 second average block time."],["TransactionByteFee",""],["Version",""]],"type":[["AccountId","Some way of identifying an account on the chain. We intentionally make it equivalent to the public key of our transaction signing scheme."],["Address","The address format for describing accounts."],["AllModules","All modules included in the runtime as a nested tuple of types. Excludes the System pallet."],["AllModulesWithSystem","All modules included in the runtime as a nested tuple of types."],["AllPallets","All pallets included in the runtime as a nested tuple of types. Excludes the System pallet."],["AllPalletsWithSystem","All pallets included in the runtime as a nested tuple of types."],["Balance","Balance of an account."],["Balances",""],["BalancesConfig",""],["Block","Block type as expected by this runtime."],["BlockNumber","An index to a block."],["Executive","Executive: handles dispatch to the various modules."],["Feeds",""],["Hash","A hash of some data used by the chain."],["Header","Block header type as expected by this runtime."],["Index","Index of a transaction in the chain."],["Moment","Type used for expressing timestamp."],["OffencesSubspace",""],["Signature","Alias to 512-bit hash when used in the context of a transaction signature on the chain."],["SignedExtra","The SignedExtension to the basic transaction logic."],["Subspace",""],["SubspaceConfig",""],["Sudo",""],["SudoConfig",""],["System",""],["SystemConfig",""],["Timestamp",""],["TransactionPayment",""],["UncheckedExtrinsic","Unchecked extrinsic type as expected by this runtime."],["Vesting",""],["VestingConfig",""]]});