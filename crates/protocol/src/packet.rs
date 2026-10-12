//! The protocol-owned packet value; its wire layout stays private to this crate.

use std::fmt;

use valentine::bedrock::version::v1_26_51::{
    self as wire, GameHeader, McpePacket, McpePacketData, McpePacketName,
};

/// One complete Bedrock 1.26.50 (protocol 2193) game packet, including its subclient header.
#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    pub(crate) header: GameHeader,
    pub(crate) data: McpePacketData,
}

/// Which packet a [`Packet`] is; its `Debug` form is the Bedrock packet name.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PacketKind(pub(crate) McpePacketName);

impl PacketKind {
    /// The packet's wire ID.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.0 as u32
    }
}

impl fmt::Debug for PacketKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl Packet {
    pub(crate) const fn new(header: GameHeader, data: McpePacketData) -> Self {
        Self { header, data }
    }

    /// Which packet this is, as its header names it.
    #[must_use]
    pub const fn kind(&self) -> PacketKind {
        PacketKind(self.header.id)
    }

    /// The generated value the vendored session layer sends.
    pub(crate) fn into_wire(self) -> McpePacket {
        McpePacket::new(self.header, self.data)
    }
}

impl From<McpePacket> for Packet {
    fn from(packet: McpePacket) -> Self {
        Self::new(packet.header, packet.data)
    }
}

impl From<McpePacketData> for Packet {
    fn from(data: McpePacketData) -> Self {
        McpePacket::from(data).into()
    }
}

/// Builds a packet from each generated payload with default subclients, as the codec does.
macro_rules! from_payloads {
    ($($payload:ident),* $(,)?) => {
        $(
            impl From<wire::$payload> for Packet {
                fn from(payload: wire::$payload) -> Self {
                    McpePacket::from(payload).into()
                }
            }
        )*
    };
}

from_payloads!(
    LoginPacket,
    PlayStatusPacket,
    ServerToClientHandshakePacket,
    ClientToServerHandshakePacket,
    DisconnectPacket,
    ResourcePacksInfoPacket,
    ResourcePackStackPacket,
    ResourcePackClientResponsePacket,
    TextPacket,
    SetTimePacket,
    StartGamePacket,
    AddPlayerPacket,
    AddActorPacket,
    RemoveActorPacket,
    AddItemActorPacket,
    ServerPlayerPostMovePositionPacket,
    TakeItemActorPacket,
    MoveActorAbsolutePacket,
    MovePlayerPacket,
    UpdateBlockPacket,
    AddPaintingPacket,
    LevelEventPacket,
    BlockEventPacket,
    ActorEventPacket,
    MobEffectPacket,
    UpdateAttributesPacket,
    InventoryTransactionPacket,
    MobEquipmentPacket,
    MobArmorEquipmentPacket,
    InteractPacket,
    BlockPickRequestPacket,
    ActorPickRequestPacket,
    PlayerActionPacket,
    HurtArmorPacket,
    SetActorDataPacket,
    SetActorMotionPacket,
    SetActorLinkPacket,
    SetHealthPacket,
    SetSpawnPositionPacket,
    AnimatePacket,
    RespawnPacket,
    ContainerOpenPacket,
    ContainerClosePacket,
    PlayerHotbarPacket,
    InventoryContentPacket,
    InventorySlotPacket,
    ContainerSetDataPacket,
    CraftingDataPacket,
    GuiDataPickItemPacket,
    BlockActorDataPacket,
    LevelChunkPacket,
    SetCommandsEnabledPacket,
    SetDifficultyPacket,
    ChangeDimensionPacket,
    SetPlayerGameTypePacket,
    PlayerListPacket,
    SimpleEventPacket,
    LegacyTelemetryEventPacket,
    SpawnExperienceOrbPacket,
    ClientboundMapItemDataPacket,
    MapInfoRequestPacket,
    RequestChunkRadiusPacket,
    ChunkRadiusUpdatedPacket,
    GameRulesChangedPacket,
    CameraPacket,
    BossEventPacket,
    ShowCreditsPacket,
    AvailableCommandsPacket,
    CommandRequestPacket,
    CommandBlockUpdatePacket,
    CommandOutputPacket,
    UpdateTradePacket,
    UpdateEquipPacket,
    ResourcePackDataInfoPacket,
    ResourcePackChunkDataPacket,
    ResourcePackChunkRequestPacket,
    TransferPacket,
    PlaySoundPacket,
    StopSoundPacket,
    SetTitlePacket,
    AddBehaviorTreePacket,
    StructureBlockUpdatePacket,
    ShowStoreOfferPacket,
    PurchaseReceiptPacket,
    PlayerSkinPacket,
    SubClientLoginPacket,
    AutomationClientConnectPacket,
    SetLastHurtByPacket,
    BookEditPacket,
    NpcRequestPacket,
    PhotoTransferPacket,
    ModalFormRequestPacket,
    ModalFormResponsePacket,
    ServerSettingsRequestPacket,
    ServerSettingsResponsePacket,
    ShowProfilePacket,
    SetDefaultGameTypePacket,
    RemoveObjectivePacket,
    SetDisplayObjectivePacket,
    SetScorePacket,
    ReservedPacket109,
    UpdateBlockSyncedPacket,
    MoveActorDeltaPacket,
    SetScoreboardIdentityPacket,
    SetLocalPlayerAsInitializedPacket,
    UpdateSoftEnumPacket,
    NetworkStackLatencyPacket,
    SpawnParticleEffectPacket,
    AvailableActorIdentifiersPacket,
    NetworkChunkPublisherUpdatePacket,
    BiomeDefinitionListPacket,
    LevelSoundEventPacket,
    LevelEventGenericPacket,
    LecternUpdatePacket,
    ClientCacheStatusPacket,
    OnScreenTextureAnimationPacket,
    MapCreateLockedCopyPacket,
    StructureTemplateDataRequestPacket,
    StructureTemplateDataResponsePacket,
    ClientCacheBlobStatusPacket,
    ClientCacheMissResponsePacket,
    ReservedPacket137,
    EmotePacket,
    MultiplayerSettingsPacket,
    SettingsCommandPacket,
    AnvilDamagePacket,
    CompletedUsingItemPacket,
    NetworkSettingsPacket,
    PlayerAuthInputPacket,
    CreativeContentPacket,
    PlayerEnchantOptionsPacket,
    ItemStackRequestPacket,
    ItemStackResponsePacket,
    PlayerArmorDamagePacket,
    ReservedPacket150,
    UpdatePlayerGameTypePacket,
    EmoteListPacket,
    PositionTrackingDbServerBroadcastPacket,
    PositionTrackingDbClientRequestPacket,
    DebugInfoPacket,
    PacketViolationWarningPacket,
    MotionPredictionHintsPacket,
    AnimateEntityPacket,
    CameraShakePacket,
    PlayerFogPacket,
    CorrectPlayerMovePredictionPacket,
    ItemRegistryPacket,
    ClientboundDebugRendererPacket,
    SyncActorPropertyPacket,
    AddVolumeEntityPacket,
    RemoveVolumeEntityPacket,
    SimulationTypePacket,
    NpcDialoguePacket,
    ReservedPacket170,
    CreatePhotoPacket,
    UpdateSubChunkBlocksPacket,
    SubChunkPacket,
    SubChunkRequestPacket,
    PlayerStartItemCooldownPacket,
    ScriptMessagePacket,
    ReservedPacket178,
    TickingAreasLoadStatusPacket,
    DimensionDataPacket,
    ReservedPacket181,
    ChangeMobPropertyPacket,
    ReservedPacket183,
    RequestAbilityPacket,
    RequestPermissionsPacket,
    ToastRequestPacket,
    UpdateAbilitiesPacket,
    UpdateAdventureSettingsPacket,
    DeathInfoPacket,
    EditorNetworkPacket,
    FeatureRegistryPacket,
    ServerStatsPacket,
    RequestNetworkSettingsPacket,
    GameTestRequestPacket,
    GameTestResultsPacket,
    UpdateClientInputLocksPacket,
    CameraPresetsPacket,
    UnlockedRecipesPacket,
    CameraInstructionPacket,
    TrimDataPacket,
    OpenSignPacket,
    ReservedPacket304,
    RefreshEntitlementsPacket,
    PlayerToggleCrafterSlotRequestPacket,
    SetPlayerInventoryOptionsPacket,
    SetHudPacket,
    AwardAchievementPacket,
    ClientboundCloseFormPacket,
    ServerboundLoadingScreenPacket,
    JigsawStructureDataPacket,
    CurrentStructureFeaturePacket,
    ServerboundDiagnosticsPacket,
    CameraAimAssistPacket,
    ContainerRegistryCleanupPacket,
    MovementEffectPacket,
    CameraAimAssistPresetsPacket,
    ClientCameraAimAssistPacket,
    ClientMovementPredictionSyncPacket,
    UpdateClientOptionsPacket,
    PlayerVideoCapturePacket,
    PlayerUpdateEntityOverridesPacket,
    PlayerLocationPacket,
    ClientboundControlSchemeSetPacket,
    PrimitiveShapesPacket,
    ServerboundPackSettingChangePacket,
    ClientboundDataStorePacket,
    GraphicsOverrideParameterPacket,
    ServerboundDataStorePacket,
    ClientboundDataDrivenUiShowScreenPacket,
    ClientboundDataDrivenUiCloseScreenPacket,
    ClientboundDataDrivenUiReloadPacket,
    ClientboundTextureShiftPacket,
    VoxelShapesPacket,
    CameraSplinePacket,
    CameraAimAssistActorPriorityPacket,
    ResourcePacksReadyForValidationPacket,
    LocatorBarPacket,
    PartyChangedPacket,
    ServerboundDataDrivenScreenClosedPacket,
    SyncWorldClocksPacket,
    ClientboundAttributeLayerSyncPacket,
    ServerStoreInfoPacket,
    ServerPresenceInfoPacket,
    ClientboundUpdateSoundDataPacket,
    SendPartyDestinationCookiePacket,
    PartyDestinationCookieResponsePacket,
    SetPlayerFurnaceOptionsPacket,
    RecordStartedPacket,
    McpePacketData,
);
