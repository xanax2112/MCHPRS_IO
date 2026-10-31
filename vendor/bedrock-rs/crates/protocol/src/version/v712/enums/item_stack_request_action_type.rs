use crate::ProtoVersion;
use bedrock_macros::ProtoCodec;

#[derive(ProtoCodec, Clone, Debug)]
#[enum_repr(i8)]
#[repr(i8)]
pub enum ItemStackRequestActionType<V: ProtoVersion> {
    Take {
        amount: i8,
        source: V::ItemStackRequestSlotInfo,
        destination: V::ItemStackRequestSlotInfo,
    } = 0,
    Place {
        amount: i8,
        source: V::ItemStackRequestSlotInfo,
        destination: V::ItemStackRequestSlotInfo,
    } = 1,
    Swap {
        source: V::ItemStackRequestSlotInfo,
        destination: V::ItemStackRequestSlotInfo,
    } = 2,
    Drop {
        amount: i8,
        source: V::ItemStackRequestSlotInfo,
        randomly: bool,
    } = 3,
    Destroy {
        amount: i8,
        source: V::ItemStackRequestSlotInfo,
    } = 4,
    Consume {
        amount: i8,
        source: V::ItemStackRequestSlotInfo,
    } = 5,
    Create {
        slot: i8,
    } = 6,
    #[deprecated]
    PlaceInItemContainer = 7,
    #[deprecated]
    TakeFromItemContainer = 8,
    ScreenLabTableCombine = 9,
    ScreenBeaconPayment {
        #[endianness(var)]
        primary_effect: i32,
        #[endianness(var)]
        secondary_effect: i32,
    } = 10,
    ScreenHUDMineBlock {
        #[endianness(var)]
        hotbar_slot: i32,
        #[endianness(var)]
        predicted_durability: i32,
        #[endianness(var)]
        stack_network_id: i32,
    } = 11,
    CraftRecipe {
        #[endianness(var)]
        recipe_network_id: i32,
        number_of_requested_crafts: i8,
    } = 12,
    CraftRecipeAuto {
        #[endianness(var)]
        recipe_network_id: i32,
        number_of_requested_crafts: i8,
        times_crafted: i8,
        #[vec_repr(u8)]
        ingredients: Vec<V::ItemDescriptorType>,
    } = 13,
    // PATCH-007（本项目本地补丁，见 docs/bedrock-integration.md）：
    // `V766` 的 `ItemStackRequestActionType` 实际用的就是 v712 这一份。
    // `creative_item_network_id` 上游写成 `i32`（zigzag varint），但 Cloudburst 766 写的是
    // **无符号** varint（id=128 → `8001`，zigzag 会是 `8002`）。宽度由
    // `refs/ProbeCraftCreative.java` 让 Cloudburst 自己编码反推：
    // 第二个字段 1 字节（127→`7f`、128→`80`、300 被截成 `2c`）。
    CraftCreative {
        #[endianness(var)]
        creative_item_network_id: u32,
        number_of_requested_crafts: u8,
    } = 14,
    CraftRecipeOptional {
        #[endianness(var)]
        recipe_network_id: i32,
        #[endianness(le)]
        filtered_strings_index: i32,
    } = 15,
    CraftRepairAndDisenchant {
        #[endianness(var)]
        recipe_network_id: i32,
        number_of_requested_crafts: i8,
        #[endianness(var)]
        repair_cost: i32,
    } = 16,
    CraftLoom {
        pattern_id: String,
        number_of_requested_crafts: i8,
    } = 17,
    #[deprecated = "Ask Tylaing"]
    CraftNonImplemented = 18,
    #[deprecated = "Ask Tylaing"]
    CraftResults {
        // TODO:
        // #[vec_repr(i32)]
        // #[vec_endianness(var)]
        // result_items: Vec<Item>
        // times_crafted: i8,
    } = 19,
}
