initSidebarItems({"enum":[["Vote","Farmer vote."]],"fn":[["derive_randomness","Derive on-chain randomness from tag signature."],["is_equivocation_proof_valid","Verifies the equivocation proof by making sure that: both headers have different hashes, are targeting the same slot, and have valid signatures by the same authority."]],"mod":[["digests","Private implementation details of Subspace consensus digests."],["inherents","Inherents for Subspace consensus"],["offence","Common traits and types that are useful for describing offences for usage in environments that use subspace consensus."],["verification","Verification for Subspace headers."]],"struct":[["GlobalRandomnesses","Subspace global randomnesses used for deriving global challenges."],["Salts","Subspace salts used for challenges."],["SignedVote","Signed farmer vote."],["SolutionRanges","Subspace solution ranges used for challenges."]],"trait":[["SubspaceApi","API necessary for block authorship with Subspace."]],"type":[["EquivocationProof","An equivocation proof for multiple block authorships on the same slot (i.e. double vote)."],["FarmerPublicKey","A Subspace farmer identifier. Necessarily equivalent to the schnorrkel public key used in the main Subspace module. If that ever changes, then this must, too."],["FarmerSignature","A Subspace farmer signature."]]});