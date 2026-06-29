#[allow(nonstandard_style)]
// Generated from grammars/PlSqlParser.g4 by ANTLR 4.8
use antlr_rust::tree::ParseTreeListener;
use super::plsqlparser::*;

pub trait PlSqlParserListener<'input> : ParseTreeListener<'input,PlSqlParserContextType>{
/**
 * Enter a parse tree produced by {@link PlSqlParser#sql_script}.
 * @param ctx the parse tree
 */
fn enter_sql_script(&mut self, _ctx: &Sql_scriptContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sql_script}.
 * @param ctx the parse tree
 */
fn exit_sql_script(&mut self, _ctx: &Sql_scriptContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unit_statement}.
 * @param ctx the parse tree
 */
fn enter_unit_statement(&mut self, _ctx: &Unit_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unit_statement}.
 * @param ctx the parse tree
 */
fn exit_unit_statement(&mut self, _ctx: &Unit_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_diskgroup}.
 * @param ctx the parse tree
 */
fn enter_alter_diskgroup(&mut self, _ctx: &Alter_diskgroupContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_diskgroup}.
 * @param ctx the parse tree
 */
fn exit_alter_diskgroup(&mut self, _ctx: &Alter_diskgroupContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_disk_clause}.
 * @param ctx the parse tree
 */
fn enter_add_disk_clause(&mut self, _ctx: &Add_disk_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_disk_clause}.
 * @param ctx the parse tree
 */
fn exit_add_disk_clause(&mut self, _ctx: &Add_disk_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_disk_clause}.
 * @param ctx the parse tree
 */
fn enter_drop_disk_clause(&mut self, _ctx: &Drop_disk_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_disk_clause}.
 * @param ctx the parse tree
 */
fn exit_drop_disk_clause(&mut self, _ctx: &Drop_disk_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#resize_disk_clause}.
 * @param ctx the parse tree
 */
fn enter_resize_disk_clause(&mut self, _ctx: &Resize_disk_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#resize_disk_clause}.
 * @param ctx the parse tree
 */
fn exit_resize_disk_clause(&mut self, _ctx: &Resize_disk_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#replace_disk_clause}.
 * @param ctx the parse tree
 */
fn enter_replace_disk_clause(&mut self, _ctx: &Replace_disk_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#replace_disk_clause}.
 * @param ctx the parse tree
 */
fn exit_replace_disk_clause(&mut self, _ctx: &Replace_disk_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#wait_nowait}.
 * @param ctx the parse tree
 */
fn enter_wait_nowait(&mut self, _ctx: &Wait_nowaitContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#wait_nowait}.
 * @param ctx the parse tree
 */
fn exit_wait_nowait(&mut self, _ctx: &Wait_nowaitContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rename_disk_clause}.
 * @param ctx the parse tree
 */
fn enter_rename_disk_clause(&mut self, _ctx: &Rename_disk_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rename_disk_clause}.
 * @param ctx the parse tree
 */
fn exit_rename_disk_clause(&mut self, _ctx: &Rename_disk_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#disk_online_clause}.
 * @param ctx the parse tree
 */
fn enter_disk_online_clause(&mut self, _ctx: &Disk_online_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#disk_online_clause}.
 * @param ctx the parse tree
 */
fn exit_disk_online_clause(&mut self, _ctx: &Disk_online_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#disk_offline_clause}.
 * @param ctx the parse tree
 */
fn enter_disk_offline_clause(&mut self, _ctx: &Disk_offline_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#disk_offline_clause}.
 * @param ctx the parse tree
 */
fn exit_disk_offline_clause(&mut self, _ctx: &Disk_offline_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#timeout_clause}.
 * @param ctx the parse tree
 */
fn enter_timeout_clause(&mut self, _ctx: &Timeout_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#timeout_clause}.
 * @param ctx the parse tree
 */
fn exit_timeout_clause(&mut self, _ctx: &Timeout_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rebalance_diskgroup_clause}.
 * @param ctx the parse tree
 */
fn enter_rebalance_diskgroup_clause(&mut self, _ctx: &Rebalance_diskgroup_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rebalance_diskgroup_clause}.
 * @param ctx the parse tree
 */
fn exit_rebalance_diskgroup_clause(&mut self, _ctx: &Rebalance_diskgroup_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#phase}.
 * @param ctx the parse tree
 */
fn enter_phase(&mut self, _ctx: &PhaseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#phase}.
 * @param ctx the parse tree
 */
fn exit_phase(&mut self, _ctx: &PhaseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#check_diskgroup_clause}.
 * @param ctx the parse tree
 */
fn enter_check_diskgroup_clause(&mut self, _ctx: &Check_diskgroup_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#check_diskgroup_clause}.
 * @param ctx the parse tree
 */
fn exit_check_diskgroup_clause(&mut self, _ctx: &Check_diskgroup_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#diskgroup_template_clauses}.
 * @param ctx the parse tree
 */
fn enter_diskgroup_template_clauses(&mut self, _ctx: &Diskgroup_template_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#diskgroup_template_clauses}.
 * @param ctx the parse tree
 */
fn exit_diskgroup_template_clauses(&mut self, _ctx: &Diskgroup_template_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#qualified_template_clause}.
 * @param ctx the parse tree
 */
fn enter_qualified_template_clause(&mut self, _ctx: &Qualified_template_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#qualified_template_clause}.
 * @param ctx the parse tree
 */
fn exit_qualified_template_clause(&mut self, _ctx: &Qualified_template_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#redundancy_clause}.
 * @param ctx the parse tree
 */
fn enter_redundancy_clause(&mut self, _ctx: &Redundancy_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#redundancy_clause}.
 * @param ctx the parse tree
 */
fn exit_redundancy_clause(&mut self, _ctx: &Redundancy_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#striping_clause}.
 * @param ctx the parse tree
 */
fn enter_striping_clause(&mut self, _ctx: &Striping_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#striping_clause}.
 * @param ctx the parse tree
 */
fn exit_striping_clause(&mut self, _ctx: &Striping_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#force_noforce}.
 * @param ctx the parse tree
 */
fn enter_force_noforce(&mut self, _ctx: &Force_noforceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#force_noforce}.
 * @param ctx the parse tree
 */
fn exit_force_noforce(&mut self, _ctx: &Force_noforceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#diskgroup_directory_clauses}.
 * @param ctx the parse tree
 */
fn enter_diskgroup_directory_clauses(&mut self, _ctx: &Diskgroup_directory_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#diskgroup_directory_clauses}.
 * @param ctx the parse tree
 */
fn exit_diskgroup_directory_clauses(&mut self, _ctx: &Diskgroup_directory_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dir_name}.
 * @param ctx the parse tree
 */
fn enter_dir_name(&mut self, _ctx: &Dir_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dir_name}.
 * @param ctx the parse tree
 */
fn exit_dir_name(&mut self, _ctx: &Dir_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#diskgroup_alias_clauses}.
 * @param ctx the parse tree
 */
fn enter_diskgroup_alias_clauses(&mut self, _ctx: &Diskgroup_alias_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#diskgroup_alias_clauses}.
 * @param ctx the parse tree
 */
fn exit_diskgroup_alias_clauses(&mut self, _ctx: &Diskgroup_alias_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#diskgroup_volume_clauses}.
 * @param ctx the parse tree
 */
fn enter_diskgroup_volume_clauses(&mut self, _ctx: &Diskgroup_volume_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#diskgroup_volume_clauses}.
 * @param ctx the parse tree
 */
fn exit_diskgroup_volume_clauses(&mut self, _ctx: &Diskgroup_volume_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_volume_clause}.
 * @param ctx the parse tree
 */
fn enter_add_volume_clause(&mut self, _ctx: &Add_volume_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_volume_clause}.
 * @param ctx the parse tree
 */
fn exit_add_volume_clause(&mut self, _ctx: &Add_volume_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_volume_clause}.
 * @param ctx the parse tree
 */
fn enter_modify_volume_clause(&mut self, _ctx: &Modify_volume_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_volume_clause}.
 * @param ctx the parse tree
 */
fn exit_modify_volume_clause(&mut self, _ctx: &Modify_volume_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#diskgroup_attributes}.
 * @param ctx the parse tree
 */
fn enter_diskgroup_attributes(&mut self, _ctx: &Diskgroup_attributesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#diskgroup_attributes}.
 * @param ctx the parse tree
 */
fn exit_diskgroup_attributes(&mut self, _ctx: &Diskgroup_attributesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_diskgroup_file_clause}.
 * @param ctx the parse tree
 */
fn enter_drop_diskgroup_file_clause(&mut self, _ctx: &Drop_diskgroup_file_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_diskgroup_file_clause}.
 * @param ctx the parse tree
 */
fn exit_drop_diskgroup_file_clause(&mut self, _ctx: &Drop_diskgroup_file_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#convert_redundancy_clause}.
 * @param ctx the parse tree
 */
fn enter_convert_redundancy_clause(&mut self, _ctx: &Convert_redundancy_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#convert_redundancy_clause}.
 * @param ctx the parse tree
 */
fn exit_convert_redundancy_clause(&mut self, _ctx: &Convert_redundancy_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#usergroup_clauses}.
 * @param ctx the parse tree
 */
fn enter_usergroup_clauses(&mut self, _ctx: &Usergroup_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#usergroup_clauses}.
 * @param ctx the parse tree
 */
fn exit_usergroup_clauses(&mut self, _ctx: &Usergroup_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#user_clauses}.
 * @param ctx the parse tree
 */
fn enter_user_clauses(&mut self, _ctx: &User_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#user_clauses}.
 * @param ctx the parse tree
 */
fn exit_user_clauses(&mut self, _ctx: &User_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#file_permissions_clause}.
 * @param ctx the parse tree
 */
fn enter_file_permissions_clause(&mut self, _ctx: &File_permissions_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#file_permissions_clause}.
 * @param ctx the parse tree
 */
fn exit_file_permissions_clause(&mut self, _ctx: &File_permissions_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#file_owner_clause}.
 * @param ctx the parse tree
 */
fn enter_file_owner_clause(&mut self, _ctx: &File_owner_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#file_owner_clause}.
 * @param ctx the parse tree
 */
fn exit_file_owner_clause(&mut self, _ctx: &File_owner_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#scrub_clause}.
 * @param ctx the parse tree
 */
fn enter_scrub_clause(&mut self, _ctx: &Scrub_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#scrub_clause}.
 * @param ctx the parse tree
 */
fn exit_scrub_clause(&mut self, _ctx: &Scrub_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#quotagroup_clauses}.
 * @param ctx the parse tree
 */
fn enter_quotagroup_clauses(&mut self, _ctx: &Quotagroup_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#quotagroup_clauses}.
 * @param ctx the parse tree
 */
fn exit_quotagroup_clauses(&mut self, _ctx: &Quotagroup_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#property_name}.
 * @param ctx the parse tree
 */
fn enter_property_name(&mut self, _ctx: &Property_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#property_name}.
 * @param ctx the parse tree
 */
fn exit_property_name(&mut self, _ctx: &Property_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#property_value}.
 * @param ctx the parse tree
 */
fn enter_property_value(&mut self, _ctx: &Property_valueContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#property_value}.
 * @param ctx the parse tree
 */
fn exit_property_value(&mut self, _ctx: &Property_valueContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#filegroup_clauses}.
 * @param ctx the parse tree
 */
fn enter_filegroup_clauses(&mut self, _ctx: &Filegroup_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#filegroup_clauses}.
 * @param ctx the parse tree
 */
fn exit_filegroup_clauses(&mut self, _ctx: &Filegroup_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_filegroup_clause}.
 * @param ctx the parse tree
 */
fn enter_add_filegroup_clause(&mut self, _ctx: &Add_filegroup_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_filegroup_clause}.
 * @param ctx the parse tree
 */
fn exit_add_filegroup_clause(&mut self, _ctx: &Add_filegroup_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_filegroup_clause}.
 * @param ctx the parse tree
 */
fn enter_modify_filegroup_clause(&mut self, _ctx: &Modify_filegroup_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_filegroup_clause}.
 * @param ctx the parse tree
 */
fn exit_modify_filegroup_clause(&mut self, _ctx: &Modify_filegroup_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#move_to_filegroup_clause}.
 * @param ctx the parse tree
 */
fn enter_move_to_filegroup_clause(&mut self, _ctx: &Move_to_filegroup_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#move_to_filegroup_clause}.
 * @param ctx the parse tree
 */
fn exit_move_to_filegroup_clause(&mut self, _ctx: &Move_to_filegroup_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_filegroup_clause}.
 * @param ctx the parse tree
 */
fn enter_drop_filegroup_clause(&mut self, _ctx: &Drop_filegroup_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_filegroup_clause}.
 * @param ctx the parse tree
 */
fn exit_drop_filegroup_clause(&mut self, _ctx: &Drop_filegroup_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#quorum_regular}.
 * @param ctx the parse tree
 */
fn enter_quorum_regular(&mut self, _ctx: &Quorum_regularContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#quorum_regular}.
 * @param ctx the parse tree
 */
fn exit_quorum_regular(&mut self, _ctx: &Quorum_regularContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#undrop_disk_clause}.
 * @param ctx the parse tree
 */
fn enter_undrop_disk_clause(&mut self, _ctx: &Undrop_disk_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#undrop_disk_clause}.
 * @param ctx the parse tree
 */
fn exit_undrop_disk_clause(&mut self, _ctx: &Undrop_disk_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#diskgroup_availability}.
 * @param ctx the parse tree
 */
fn enter_diskgroup_availability(&mut self, _ctx: &Diskgroup_availabilityContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#diskgroup_availability}.
 * @param ctx the parse tree
 */
fn exit_diskgroup_availability(&mut self, _ctx: &Diskgroup_availabilityContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#enable_disable_volume}.
 * @param ctx the parse tree
 */
fn enter_enable_disable_volume(&mut self, _ctx: &Enable_disable_volumeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#enable_disable_volume}.
 * @param ctx the parse tree
 */
fn exit_enable_disable_volume(&mut self, _ctx: &Enable_disable_volumeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_function}.
 * @param ctx the parse tree
 */
fn enter_drop_function(&mut self, _ctx: &Drop_functionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_function}.
 * @param ctx the parse tree
 */
fn exit_drop_function(&mut self, _ctx: &Drop_functionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_flashback_archive}.
 * @param ctx the parse tree
 */
fn enter_alter_flashback_archive(&mut self, _ctx: &Alter_flashback_archiveContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_flashback_archive}.
 * @param ctx the parse tree
 */
fn exit_alter_flashback_archive(&mut self, _ctx: &Alter_flashback_archiveContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_hierarchy}.
 * @param ctx the parse tree
 */
fn enter_alter_hierarchy(&mut self, _ctx: &Alter_hierarchyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_hierarchy}.
 * @param ctx the parse tree
 */
fn exit_alter_hierarchy(&mut self, _ctx: &Alter_hierarchyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_function}.
 * @param ctx the parse tree
 */
fn enter_alter_function(&mut self, _ctx: &Alter_functionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_function}.
 * @param ctx the parse tree
 */
fn exit_alter_function(&mut self, _ctx: &Alter_functionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_java}.
 * @param ctx the parse tree
 */
fn enter_alter_java(&mut self, _ctx: &Alter_javaContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_java}.
 * @param ctx the parse tree
 */
fn exit_alter_java(&mut self, _ctx: &Alter_javaContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#match_string}.
 * @param ctx the parse tree
 */
fn enter_match_string(&mut self, _ctx: &Match_stringContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#match_string}.
 * @param ctx the parse tree
 */
fn exit_match_string(&mut self, _ctx: &Match_stringContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_function_body}.
 * @param ctx the parse tree
 */
fn enter_create_function_body(&mut self, _ctx: &Create_function_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_function_body}.
 * @param ctx the parse tree
 */
fn exit_create_function_body(&mut self, _ctx: &Create_function_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sql_macro_body}.
 * @param ctx the parse tree
 */
fn enter_sql_macro_body(&mut self, _ctx: &Sql_macro_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sql_macro_body}.
 * @param ctx the parse tree
 */
fn exit_sql_macro_body(&mut self, _ctx: &Sql_macro_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#parallel_enable_clause}.
 * @param ctx the parse tree
 */
fn enter_parallel_enable_clause(&mut self, _ctx: &Parallel_enable_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#parallel_enable_clause}.
 * @param ctx the parse tree
 */
fn exit_parallel_enable_clause(&mut self, _ctx: &Parallel_enable_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partition_by_clause}.
 * @param ctx the parse tree
 */
fn enter_partition_by_clause(&mut self, _ctx: &Partition_by_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partition_by_clause}.
 * @param ctx the parse tree
 */
fn exit_partition_by_clause(&mut self, _ctx: &Partition_by_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#result_cache_clause}.
 * @param ctx the parse tree
 */
fn enter_result_cache_clause(&mut self, _ctx: &Result_cache_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#result_cache_clause}.
 * @param ctx the parse tree
 */
fn exit_result_cache_clause(&mut self, _ctx: &Result_cache_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#accessible_by_clause}.
 * @param ctx the parse tree
 */
fn enter_accessible_by_clause(&mut self, _ctx: &Accessible_by_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#accessible_by_clause}.
 * @param ctx the parse tree
 */
fn exit_accessible_by_clause(&mut self, _ctx: &Accessible_by_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_collation_clause}.
 * @param ctx the parse tree
 */
fn enter_default_collation_clause(&mut self, _ctx: &Default_collation_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_collation_clause}.
 * @param ctx the parse tree
 */
fn exit_default_collation_clause(&mut self, _ctx: &Default_collation_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#aggregate_clause}.
 * @param ctx the parse tree
 */
fn enter_aggregate_clause(&mut self, _ctx: &Aggregate_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#aggregate_clause}.
 * @param ctx the parse tree
 */
fn exit_aggregate_clause(&mut self, _ctx: &Aggregate_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pipelined_using_clause}.
 * @param ctx the parse tree
 */
fn enter_pipelined_using_clause(&mut self, _ctx: &Pipelined_using_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pipelined_using_clause}.
 * @param ctx the parse tree
 */
fn exit_pipelined_using_clause(&mut self, _ctx: &Pipelined_using_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#accessor}.
 * @param ctx the parse tree
 */
fn enter_accessor(&mut self, _ctx: &AccessorContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#accessor}.
 * @param ctx the parse tree
 */
fn exit_accessor(&mut self, _ctx: &AccessorContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#relies_on_part}.
 * @param ctx the parse tree
 */
fn enter_relies_on_part(&mut self, _ctx: &Relies_on_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#relies_on_part}.
 * @param ctx the parse tree
 */
fn exit_relies_on_part(&mut self, _ctx: &Relies_on_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#streaming_clause}.
 * @param ctx the parse tree
 */
fn enter_streaming_clause(&mut self, _ctx: &Streaming_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#streaming_clause}.
 * @param ctx the parse tree
 */
fn exit_streaming_clause(&mut self, _ctx: &Streaming_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_outline}.
 * @param ctx the parse tree
 */
fn enter_alter_outline(&mut self, _ctx: &Alter_outlineContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_outline}.
 * @param ctx the parse tree
 */
fn exit_alter_outline(&mut self, _ctx: &Alter_outlineContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#outline_options}.
 * @param ctx the parse tree
 */
fn enter_outline_options(&mut self, _ctx: &Outline_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#outline_options}.
 * @param ctx the parse tree
 */
fn exit_outline_options(&mut self, _ctx: &Outline_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_lockdown_profile}.
 * @param ctx the parse tree
 */
fn enter_alter_lockdown_profile(&mut self, _ctx: &Alter_lockdown_profileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_lockdown_profile}.
 * @param ctx the parse tree
 */
fn exit_alter_lockdown_profile(&mut self, _ctx: &Alter_lockdown_profileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lockdown_feature}.
 * @param ctx the parse tree
 */
fn enter_lockdown_feature(&mut self, _ctx: &Lockdown_featureContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lockdown_feature}.
 * @param ctx the parse tree
 */
fn exit_lockdown_feature(&mut self, _ctx: &Lockdown_featureContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lockdown_options}.
 * @param ctx the parse tree
 */
fn enter_lockdown_options(&mut self, _ctx: &Lockdown_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lockdown_options}.
 * @param ctx the parse tree
 */
fn exit_lockdown_options(&mut self, _ctx: &Lockdown_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lockdown_statements}.
 * @param ctx the parse tree
 */
fn enter_lockdown_statements(&mut self, _ctx: &Lockdown_statementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lockdown_statements}.
 * @param ctx the parse tree
 */
fn exit_lockdown_statements(&mut self, _ctx: &Lockdown_statementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#statement_clauses}.
 * @param ctx the parse tree
 */
fn enter_statement_clauses(&mut self, _ctx: &Statement_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#statement_clauses}.
 * @param ctx the parse tree
 */
fn exit_statement_clauses(&mut self, _ctx: &Statement_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#clause_options}.
 * @param ctx the parse tree
 */
fn enter_clause_options(&mut self, _ctx: &Clause_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#clause_options}.
 * @param ctx the parse tree
 */
fn exit_clause_options(&mut self, _ctx: &Clause_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#option_values}.
 * @param ctx the parse tree
 */
fn enter_option_values(&mut self, _ctx: &Option_valuesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#option_values}.
 * @param ctx the parse tree
 */
fn exit_option_values(&mut self, _ctx: &Option_valuesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#string_list}.
 * @param ctx the parse tree
 */
fn enter_string_list(&mut self, _ctx: &String_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#string_list}.
 * @param ctx the parse tree
 */
fn exit_string_list(&mut self, _ctx: &String_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#disable_enable}.
 * @param ctx the parse tree
 */
fn enter_disable_enable(&mut self, _ctx: &Disable_enableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#disable_enable}.
 * @param ctx the parse tree
 */
fn exit_disable_enable(&mut self, _ctx: &Disable_enableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_lockdown_profile}.
 * @param ctx the parse tree
 */
fn enter_drop_lockdown_profile(&mut self, _ctx: &Drop_lockdown_profileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_lockdown_profile}.
 * @param ctx the parse tree
 */
fn exit_drop_lockdown_profile(&mut self, _ctx: &Drop_lockdown_profileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_package}.
 * @param ctx the parse tree
 */
fn enter_drop_package(&mut self, _ctx: &Drop_packageContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_package}.
 * @param ctx the parse tree
 */
fn exit_drop_package(&mut self, _ctx: &Drop_packageContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_package}.
 * @param ctx the parse tree
 */
fn enter_alter_package(&mut self, _ctx: &Alter_packageContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_package}.
 * @param ctx the parse tree
 */
fn exit_alter_package(&mut self, _ctx: &Alter_packageContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_package}.
 * @param ctx the parse tree
 */
fn enter_create_package(&mut self, _ctx: &Create_packageContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_package}.
 * @param ctx the parse tree
 */
fn exit_create_package(&mut self, _ctx: &Create_packageContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_package_body}.
 * @param ctx the parse tree
 */
fn enter_create_package_body(&mut self, _ctx: &Create_package_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_package_body}.
 * @param ctx the parse tree
 */
fn exit_create_package_body(&mut self, _ctx: &Create_package_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#package_obj_spec}.
 * @param ctx the parse tree
 */
fn enter_package_obj_spec(&mut self, _ctx: &Package_obj_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#package_obj_spec}.
 * @param ctx the parse tree
 */
fn exit_package_obj_spec(&mut self, _ctx: &Package_obj_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#procedure_spec}.
 * @param ctx the parse tree
 */
fn enter_procedure_spec(&mut self, _ctx: &Procedure_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#procedure_spec}.
 * @param ctx the parse tree
 */
fn exit_procedure_spec(&mut self, _ctx: &Procedure_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#function_spec}.
 * @param ctx the parse tree
 */
fn enter_function_spec(&mut self, _ctx: &Function_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#function_spec}.
 * @param ctx the parse tree
 */
fn exit_function_spec(&mut self, _ctx: &Function_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#package_obj_body}.
 * @param ctx the parse tree
 */
fn enter_package_obj_body(&mut self, _ctx: &Package_obj_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#package_obj_body}.
 * @param ctx the parse tree
 */
fn exit_package_obj_body(&mut self, _ctx: &Package_obj_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_pmem_filestore}.
 * @param ctx the parse tree
 */
fn enter_alter_pmem_filestore(&mut self, _ctx: &Alter_pmem_filestoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_pmem_filestore}.
 * @param ctx the parse tree
 */
fn exit_alter_pmem_filestore(&mut self, _ctx: &Alter_pmem_filestoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_pmem_filestore}.
 * @param ctx the parse tree
 */
fn enter_drop_pmem_filestore(&mut self, _ctx: &Drop_pmem_filestoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_pmem_filestore}.
 * @param ctx the parse tree
 */
fn exit_drop_pmem_filestore(&mut self, _ctx: &Drop_pmem_filestoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_procedure}.
 * @param ctx the parse tree
 */
fn enter_drop_procedure(&mut self, _ctx: &Drop_procedureContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_procedure}.
 * @param ctx the parse tree
 */
fn exit_drop_procedure(&mut self, _ctx: &Drop_procedureContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_procedure}.
 * @param ctx the parse tree
 */
fn enter_alter_procedure(&mut self, _ctx: &Alter_procedureContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_procedure}.
 * @param ctx the parse tree
 */
fn exit_alter_procedure(&mut self, _ctx: &Alter_procedureContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#function_body}.
 * @param ctx the parse tree
 */
fn enter_function_body(&mut self, _ctx: &Function_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#function_body}.
 * @param ctx the parse tree
 */
fn exit_function_body(&mut self, _ctx: &Function_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#procedure_body}.
 * @param ctx the parse tree
 */
fn enter_procedure_body(&mut self, _ctx: &Procedure_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#procedure_body}.
 * @param ctx the parse tree
 */
fn exit_procedure_body(&mut self, _ctx: &Procedure_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_procedure_body}.
 * @param ctx the parse tree
 */
fn enter_create_procedure_body(&mut self, _ctx: &Create_procedure_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_procedure_body}.
 * @param ctx the parse tree
 */
fn exit_create_procedure_body(&mut self, _ctx: &Create_procedure_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_resource_cost}.
 * @param ctx the parse tree
 */
fn enter_alter_resource_cost(&mut self, _ctx: &Alter_resource_costContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_resource_cost}.
 * @param ctx the parse tree
 */
fn exit_alter_resource_cost(&mut self, _ctx: &Alter_resource_costContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_outline}.
 * @param ctx the parse tree
 */
fn enter_drop_outline(&mut self, _ctx: &Drop_outlineContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_outline}.
 * @param ctx the parse tree
 */
fn exit_drop_outline(&mut self, _ctx: &Drop_outlineContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_rollback_segment}.
 * @param ctx the parse tree
 */
fn enter_alter_rollback_segment(&mut self, _ctx: &Alter_rollback_segmentContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_rollback_segment}.
 * @param ctx the parse tree
 */
fn exit_alter_rollback_segment(&mut self, _ctx: &Alter_rollback_segmentContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_restore_point}.
 * @param ctx the parse tree
 */
fn enter_drop_restore_point(&mut self, _ctx: &Drop_restore_pointContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_restore_point}.
 * @param ctx the parse tree
 */
fn exit_drop_restore_point(&mut self, _ctx: &Drop_restore_pointContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_rollback_segment}.
 * @param ctx the parse tree
 */
fn enter_drop_rollback_segment(&mut self, _ctx: &Drop_rollback_segmentContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_rollback_segment}.
 * @param ctx the parse tree
 */
fn exit_drop_rollback_segment(&mut self, _ctx: &Drop_rollback_segmentContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_role}.
 * @param ctx the parse tree
 */
fn enter_drop_role(&mut self, _ctx: &Drop_roleContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_role}.
 * @param ctx the parse tree
 */
fn exit_drop_role(&mut self, _ctx: &Drop_roleContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_pmem_filestore}.
 * @param ctx the parse tree
 */
fn enter_create_pmem_filestore(&mut self, _ctx: &Create_pmem_filestoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_pmem_filestore}.
 * @param ctx the parse tree
 */
fn exit_create_pmem_filestore(&mut self, _ctx: &Create_pmem_filestoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pmem_filestore_options}.
 * @param ctx the parse tree
 */
fn enter_pmem_filestore_options(&mut self, _ctx: &Pmem_filestore_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pmem_filestore_options}.
 * @param ctx the parse tree
 */
fn exit_pmem_filestore_options(&mut self, _ctx: &Pmem_filestore_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#file_path}.
 * @param ctx the parse tree
 */
fn enter_file_path(&mut self, _ctx: &File_pathContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#file_path}.
 * @param ctx the parse tree
 */
fn exit_file_path(&mut self, _ctx: &File_pathContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_rollback_segment}.
 * @param ctx the parse tree
 */
fn enter_create_rollback_segment(&mut self, _ctx: &Create_rollback_segmentContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_rollback_segment}.
 * @param ctx the parse tree
 */
fn exit_create_rollback_segment(&mut self, _ctx: &Create_rollback_segmentContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_trigger}.
 * @param ctx the parse tree
 */
fn enter_drop_trigger(&mut self, _ctx: &Drop_triggerContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_trigger}.
 * @param ctx the parse tree
 */
fn exit_drop_trigger(&mut self, _ctx: &Drop_triggerContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_trigger}.
 * @param ctx the parse tree
 */
fn enter_alter_trigger(&mut self, _ctx: &Alter_triggerContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_trigger}.
 * @param ctx the parse tree
 */
fn exit_alter_trigger(&mut self, _ctx: &Alter_triggerContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_trigger}.
 * @param ctx the parse tree
 */
fn enter_create_trigger(&mut self, _ctx: &Create_triggerContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_trigger}.
 * @param ctx the parse tree
 */
fn exit_create_trigger(&mut self, _ctx: &Create_triggerContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#trigger_follows_clause}.
 * @param ctx the parse tree
 */
fn enter_trigger_follows_clause(&mut self, _ctx: &Trigger_follows_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#trigger_follows_clause}.
 * @param ctx the parse tree
 */
fn exit_trigger_follows_clause(&mut self, _ctx: &Trigger_follows_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#trigger_when_clause}.
 * @param ctx the parse tree
 */
fn enter_trigger_when_clause(&mut self, _ctx: &Trigger_when_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#trigger_when_clause}.
 * @param ctx the parse tree
 */
fn exit_trigger_when_clause(&mut self, _ctx: &Trigger_when_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#simple_dml_trigger}.
 * @param ctx the parse tree
 */
fn enter_simple_dml_trigger(&mut self, _ctx: &Simple_dml_triggerContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#simple_dml_trigger}.
 * @param ctx the parse tree
 */
fn exit_simple_dml_trigger(&mut self, _ctx: &Simple_dml_triggerContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#for_each_row}.
 * @param ctx the parse tree
 */
fn enter_for_each_row(&mut self, _ctx: &For_each_rowContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#for_each_row}.
 * @param ctx the parse tree
 */
fn exit_for_each_row(&mut self, _ctx: &For_each_rowContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#compound_dml_trigger}.
 * @param ctx the parse tree
 */
fn enter_compound_dml_trigger(&mut self, _ctx: &Compound_dml_triggerContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#compound_dml_trigger}.
 * @param ctx the parse tree
 */
fn exit_compound_dml_trigger(&mut self, _ctx: &Compound_dml_triggerContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#non_dml_trigger}.
 * @param ctx the parse tree
 */
fn enter_non_dml_trigger(&mut self, _ctx: &Non_dml_triggerContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#non_dml_trigger}.
 * @param ctx the parse tree
 */
fn exit_non_dml_trigger(&mut self, _ctx: &Non_dml_triggerContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#trigger_body}.
 * @param ctx the parse tree
 */
fn enter_trigger_body(&mut self, _ctx: &Trigger_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#trigger_body}.
 * @param ctx the parse tree
 */
fn exit_trigger_body(&mut self, _ctx: &Trigger_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#compound_trigger_block}.
 * @param ctx the parse tree
 */
fn enter_compound_trigger_block(&mut self, _ctx: &Compound_trigger_blockContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#compound_trigger_block}.
 * @param ctx the parse tree
 */
fn exit_compound_trigger_block(&mut self, _ctx: &Compound_trigger_blockContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#timing_point_section}.
 * @param ctx the parse tree
 */
fn enter_timing_point_section(&mut self, _ctx: &Timing_point_sectionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#timing_point_section}.
 * @param ctx the parse tree
 */
fn exit_timing_point_section(&mut self, _ctx: &Timing_point_sectionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#non_dml_event}.
 * @param ctx the parse tree
 */
fn enter_non_dml_event(&mut self, _ctx: &Non_dml_eventContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#non_dml_event}.
 * @param ctx the parse tree
 */
fn exit_non_dml_event(&mut self, _ctx: &Non_dml_eventContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dml_event_clause}.
 * @param ctx the parse tree
 */
fn enter_dml_event_clause(&mut self, _ctx: &Dml_event_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dml_event_clause}.
 * @param ctx the parse tree
 */
fn exit_dml_event_clause(&mut self, _ctx: &Dml_event_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dml_event_element}.
 * @param ctx the parse tree
 */
fn enter_dml_event_element(&mut self, _ctx: &Dml_event_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dml_event_element}.
 * @param ctx the parse tree
 */
fn exit_dml_event_element(&mut self, _ctx: &Dml_event_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dml_event_nested_clause}.
 * @param ctx the parse tree
 */
fn enter_dml_event_nested_clause(&mut self, _ctx: &Dml_event_nested_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dml_event_nested_clause}.
 * @param ctx the parse tree
 */
fn exit_dml_event_nested_clause(&mut self, _ctx: &Dml_event_nested_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#referencing_clause}.
 * @param ctx the parse tree
 */
fn enter_referencing_clause(&mut self, _ctx: &Referencing_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#referencing_clause}.
 * @param ctx the parse tree
 */
fn exit_referencing_clause(&mut self, _ctx: &Referencing_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#referencing_element}.
 * @param ctx the parse tree
 */
fn enter_referencing_element(&mut self, _ctx: &Referencing_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#referencing_element}.
 * @param ctx the parse tree
 */
fn exit_referencing_element(&mut self, _ctx: &Referencing_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_type}.
 * @param ctx the parse tree
 */
fn enter_drop_type(&mut self, _ctx: &Drop_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_type}.
 * @param ctx the parse tree
 */
fn exit_drop_type(&mut self, _ctx: &Drop_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_type}.
 * @param ctx the parse tree
 */
fn enter_alter_type(&mut self, _ctx: &Alter_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_type}.
 * @param ctx the parse tree
 */
fn exit_alter_type(&mut self, _ctx: &Alter_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#compile_type_clause}.
 * @param ctx the parse tree
 */
fn enter_compile_type_clause(&mut self, _ctx: &Compile_type_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#compile_type_clause}.
 * @param ctx the parse tree
 */
fn exit_compile_type_clause(&mut self, _ctx: &Compile_type_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#replace_type_clause}.
 * @param ctx the parse tree
 */
fn enter_replace_type_clause(&mut self, _ctx: &Replace_type_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#replace_type_clause}.
 * @param ctx the parse tree
 */
fn exit_replace_type_clause(&mut self, _ctx: &Replace_type_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_method_spec}.
 * @param ctx the parse tree
 */
fn enter_alter_method_spec(&mut self, _ctx: &Alter_method_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_method_spec}.
 * @param ctx the parse tree
 */
fn exit_alter_method_spec(&mut self, _ctx: &Alter_method_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_method_element}.
 * @param ctx the parse tree
 */
fn enter_alter_method_element(&mut self, _ctx: &Alter_method_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_method_element}.
 * @param ctx the parse tree
 */
fn exit_alter_method_element(&mut self, _ctx: &Alter_method_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_collection_clauses}.
 * @param ctx the parse tree
 */
fn enter_alter_collection_clauses(&mut self, _ctx: &Alter_collection_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_collection_clauses}.
 * @param ctx the parse tree
 */
fn exit_alter_collection_clauses(&mut self, _ctx: &Alter_collection_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dependent_handling_clause}.
 * @param ctx the parse tree
 */
fn enter_dependent_handling_clause(&mut self, _ctx: &Dependent_handling_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dependent_handling_clause}.
 * @param ctx the parse tree
 */
fn exit_dependent_handling_clause(&mut self, _ctx: &Dependent_handling_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dependent_exceptions_part}.
 * @param ctx the parse tree
 */
fn enter_dependent_exceptions_part(&mut self, _ctx: &Dependent_exceptions_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dependent_exceptions_part}.
 * @param ctx the parse tree
 */
fn exit_dependent_exceptions_part(&mut self, _ctx: &Dependent_exceptions_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_type}.
 * @param ctx the parse tree
 */
fn enter_create_type(&mut self, _ctx: &Create_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_type}.
 * @param ctx the parse tree
 */
fn exit_create_type(&mut self, _ctx: &Create_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_definition}.
 * @param ctx the parse tree
 */
fn enter_type_definition(&mut self, _ctx: &Type_definitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_definition}.
 * @param ctx the parse tree
 */
fn exit_type_definition(&mut self, _ctx: &Type_definitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_type_def}.
 * @param ctx the parse tree
 */
fn enter_object_type_def(&mut self, _ctx: &Object_type_defContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_type_def}.
 * @param ctx the parse tree
 */
fn exit_object_type_def(&mut self, _ctx: &Object_type_defContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_as_part}.
 * @param ctx the parse tree
 */
fn enter_object_as_part(&mut self, _ctx: &Object_as_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_as_part}.
 * @param ctx the parse tree
 */
fn exit_object_as_part(&mut self, _ctx: &Object_as_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_under_part}.
 * @param ctx the parse tree
 */
fn enter_object_under_part(&mut self, _ctx: &Object_under_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_under_part}.
 * @param ctx the parse tree
 */
fn exit_object_under_part(&mut self, _ctx: &Object_under_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#nested_table_type_def}.
 * @param ctx the parse tree
 */
fn enter_nested_table_type_def(&mut self, _ctx: &Nested_table_type_defContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#nested_table_type_def}.
 * @param ctx the parse tree
 */
fn exit_nested_table_type_def(&mut self, _ctx: &Nested_table_type_defContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sqlj_object_type}.
 * @param ctx the parse tree
 */
fn enter_sqlj_object_type(&mut self, _ctx: &Sqlj_object_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sqlj_object_type}.
 * @param ctx the parse tree
 */
fn exit_sqlj_object_type(&mut self, _ctx: &Sqlj_object_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_body}.
 * @param ctx the parse tree
 */
fn enter_type_body(&mut self, _ctx: &Type_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_body}.
 * @param ctx the parse tree
 */
fn exit_type_body(&mut self, _ctx: &Type_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_body_elements}.
 * @param ctx the parse tree
 */
fn enter_type_body_elements(&mut self, _ctx: &Type_body_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_body_elements}.
 * @param ctx the parse tree
 */
fn exit_type_body_elements(&mut self, _ctx: &Type_body_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#map_order_func_declaration}.
 * @param ctx the parse tree
 */
fn enter_map_order_func_declaration(&mut self, _ctx: &Map_order_func_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#map_order_func_declaration}.
 * @param ctx the parse tree
 */
fn exit_map_order_func_declaration(&mut self, _ctx: &Map_order_func_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subprog_decl_in_type}.
 * @param ctx the parse tree
 */
fn enter_subprog_decl_in_type(&mut self, _ctx: &Subprog_decl_in_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subprog_decl_in_type}.
 * @param ctx the parse tree
 */
fn exit_subprog_decl_in_type(&mut self, _ctx: &Subprog_decl_in_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#proc_decl_in_type}.
 * @param ctx the parse tree
 */
fn enter_proc_decl_in_type(&mut self, _ctx: &Proc_decl_in_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#proc_decl_in_type}.
 * @param ctx the parse tree
 */
fn exit_proc_decl_in_type(&mut self, _ctx: &Proc_decl_in_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#func_decl_in_type}.
 * @param ctx the parse tree
 */
fn enter_func_decl_in_type(&mut self, _ctx: &Func_decl_in_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#func_decl_in_type}.
 * @param ctx the parse tree
 */
fn exit_func_decl_in_type(&mut self, _ctx: &Func_decl_in_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#constructor_declaration}.
 * @param ctx the parse tree
 */
fn enter_constructor_declaration(&mut self, _ctx: &Constructor_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#constructor_declaration}.
 * @param ctx the parse tree
 */
fn exit_constructor_declaration(&mut self, _ctx: &Constructor_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modifier_clause}.
 * @param ctx the parse tree
 */
fn enter_modifier_clause(&mut self, _ctx: &Modifier_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modifier_clause}.
 * @param ctx the parse tree
 */
fn exit_modifier_clause(&mut self, _ctx: &Modifier_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_member_spec}.
 * @param ctx the parse tree
 */
fn enter_object_member_spec(&mut self, _ctx: &Object_member_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_member_spec}.
 * @param ctx the parse tree
 */
fn exit_object_member_spec(&mut self, _ctx: &Object_member_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sqlj_object_type_attr}.
 * @param ctx the parse tree
 */
fn enter_sqlj_object_type_attr(&mut self, _ctx: &Sqlj_object_type_attrContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sqlj_object_type_attr}.
 * @param ctx the parse tree
 */
fn exit_sqlj_object_type_attr(&mut self, _ctx: &Sqlj_object_type_attrContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#element_spec}.
 * @param ctx the parse tree
 */
fn enter_element_spec(&mut self, _ctx: &Element_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#element_spec}.
 * @param ctx the parse tree
 */
fn exit_element_spec(&mut self, _ctx: &Element_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#element_spec_options}.
 * @param ctx the parse tree
 */
fn enter_element_spec_options(&mut self, _ctx: &Element_spec_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#element_spec_options}.
 * @param ctx the parse tree
 */
fn exit_element_spec_options(&mut self, _ctx: &Element_spec_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subprogram_spec}.
 * @param ctx the parse tree
 */
fn enter_subprogram_spec(&mut self, _ctx: &Subprogram_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subprogram_spec}.
 * @param ctx the parse tree
 */
fn exit_subprogram_spec(&mut self, _ctx: &Subprogram_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#overriding_subprogram_spec}.
 * @param ctx the parse tree
 */
fn enter_overriding_subprogram_spec(&mut self, _ctx: &Overriding_subprogram_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#overriding_subprogram_spec}.
 * @param ctx the parse tree
 */
fn exit_overriding_subprogram_spec(&mut self, _ctx: &Overriding_subprogram_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#overriding_function_spec}.
 * @param ctx the parse tree
 */
fn enter_overriding_function_spec(&mut self, _ctx: &Overriding_function_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#overriding_function_spec}.
 * @param ctx the parse tree
 */
fn exit_overriding_function_spec(&mut self, _ctx: &Overriding_function_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#overriding_procedure_spec}.
 * @param ctx the parse tree
 */
fn enter_overriding_procedure_spec(&mut self, _ctx: &Overriding_procedure_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#overriding_procedure_spec}.
 * @param ctx the parse tree
 */
fn exit_overriding_procedure_spec(&mut self, _ctx: &Overriding_procedure_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_procedure_spec}.
 * @param ctx the parse tree
 */
fn enter_type_procedure_spec(&mut self, _ctx: &Type_procedure_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_procedure_spec}.
 * @param ctx the parse tree
 */
fn exit_type_procedure_spec(&mut self, _ctx: &Type_procedure_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_function_spec}.
 * @param ctx the parse tree
 */
fn enter_type_function_spec(&mut self, _ctx: &Type_function_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_function_spec}.
 * @param ctx the parse tree
 */
fn exit_type_function_spec(&mut self, _ctx: &Type_function_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#constructor_spec}.
 * @param ctx the parse tree
 */
fn enter_constructor_spec(&mut self, _ctx: &Constructor_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#constructor_spec}.
 * @param ctx the parse tree
 */
fn exit_constructor_spec(&mut self, _ctx: &Constructor_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#map_order_function_spec}.
 * @param ctx the parse tree
 */
fn enter_map_order_function_spec(&mut self, _ctx: &Map_order_function_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#map_order_function_spec}.
 * @param ctx the parse tree
 */
fn exit_map_order_function_spec(&mut self, _ctx: &Map_order_function_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pragma_clause}.
 * @param ctx the parse tree
 */
fn enter_pragma_clause(&mut self, _ctx: &Pragma_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pragma_clause}.
 * @param ctx the parse tree
 */
fn exit_pragma_clause(&mut self, _ctx: &Pragma_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pragma_elements}.
 * @param ctx the parse tree
 */
fn enter_pragma_elements(&mut self, _ctx: &Pragma_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pragma_elements}.
 * @param ctx the parse tree
 */
fn exit_pragma_elements(&mut self, _ctx: &Pragma_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_elements_parameter}.
 * @param ctx the parse tree
 */
fn enter_type_elements_parameter(&mut self, _ctx: &Type_elements_parameterContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_elements_parameter}.
 * @param ctx the parse tree
 */
fn exit_type_elements_parameter(&mut self, _ctx: &Type_elements_parameterContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_sequence}.
 * @param ctx the parse tree
 */
fn enter_drop_sequence(&mut self, _ctx: &Drop_sequenceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_sequence}.
 * @param ctx the parse tree
 */
fn exit_drop_sequence(&mut self, _ctx: &Drop_sequenceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_sequence}.
 * @param ctx the parse tree
 */
fn enter_alter_sequence(&mut self, _ctx: &Alter_sequenceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_sequence}.
 * @param ctx the parse tree
 */
fn exit_alter_sequence(&mut self, _ctx: &Alter_sequenceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_session}.
 * @param ctx the parse tree
 */
fn enter_alter_session(&mut self, _ctx: &Alter_sessionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_session}.
 * @param ctx the parse tree
 */
fn exit_alter_session(&mut self, _ctx: &Alter_sessionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_session_set_clause}.
 * @param ctx the parse tree
 */
fn enter_alter_session_set_clause(&mut self, _ctx: &Alter_session_set_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_session_set_clause}.
 * @param ctx the parse tree
 */
fn exit_alter_session_set_clause(&mut self, _ctx: &Alter_session_set_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_sequence}.
 * @param ctx the parse tree
 */
fn enter_create_sequence(&mut self, _ctx: &Create_sequenceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_sequence}.
 * @param ctx the parse tree
 */
fn exit_create_sequence(&mut self, _ctx: &Create_sequenceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sequence_spec}.
 * @param ctx the parse tree
 */
fn enter_sequence_spec(&mut self, _ctx: &Sequence_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sequence_spec}.
 * @param ctx the parse tree
 */
fn exit_sequence_spec(&mut self, _ctx: &Sequence_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sequence_start_clause}.
 * @param ctx the parse tree
 */
fn enter_sequence_start_clause(&mut self, _ctx: &Sequence_start_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sequence_start_clause}.
 * @param ctx the parse tree
 */
fn exit_sequence_start_clause(&mut self, _ctx: &Sequence_start_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_analytic_view}.
 * @param ctx the parse tree
 */
fn enter_create_analytic_view(&mut self, _ctx: &Create_analytic_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_analytic_view}.
 * @param ctx the parse tree
 */
fn exit_create_analytic_view(&mut self, _ctx: &Create_analytic_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#classification_clause}.
 * @param ctx the parse tree
 */
fn enter_classification_clause(&mut self, _ctx: &Classification_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#classification_clause}.
 * @param ctx the parse tree
 */
fn exit_classification_clause(&mut self, _ctx: &Classification_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#caption_clause}.
 * @param ctx the parse tree
 */
fn enter_caption_clause(&mut self, _ctx: &Caption_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#caption_clause}.
 * @param ctx the parse tree
 */
fn exit_caption_clause(&mut self, _ctx: &Caption_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#description_clause}.
 * @param ctx the parse tree
 */
fn enter_description_clause(&mut self, _ctx: &Description_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#description_clause}.
 * @param ctx the parse tree
 */
fn exit_description_clause(&mut self, _ctx: &Description_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#classification_item}.
 * @param ctx the parse tree
 */
fn enter_classification_item(&mut self, _ctx: &Classification_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#classification_item}.
 * @param ctx the parse tree
 */
fn exit_classification_item(&mut self, _ctx: &Classification_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#language}.
 * @param ctx the parse tree
 */
fn enter_language(&mut self, _ctx: &LanguageContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#language}.
 * @param ctx the parse tree
 */
fn exit_language(&mut self, _ctx: &LanguageContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cav_using_clause}.
 * @param ctx the parse tree
 */
fn enter_cav_using_clause(&mut self, _ctx: &Cav_using_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cav_using_clause}.
 * @param ctx the parse tree
 */
fn exit_cav_using_clause(&mut self, _ctx: &Cav_using_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dim_by_clause}.
 * @param ctx the parse tree
 */
fn enter_dim_by_clause(&mut self, _ctx: &Dim_by_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dim_by_clause}.
 * @param ctx the parse tree
 */
fn exit_dim_by_clause(&mut self, _ctx: &Dim_by_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dim_key}.
 * @param ctx the parse tree
 */
fn enter_dim_key(&mut self, _ctx: &Dim_keyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dim_key}.
 * @param ctx the parse tree
 */
fn exit_dim_key(&mut self, _ctx: &Dim_keyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dim_ref}.
 * @param ctx the parse tree
 */
fn enter_dim_ref(&mut self, _ctx: &Dim_refContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dim_ref}.
 * @param ctx the parse tree
 */
fn exit_dim_ref(&mut self, _ctx: &Dim_refContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hier_ref}.
 * @param ctx the parse tree
 */
fn enter_hier_ref(&mut self, _ctx: &Hier_refContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hier_ref}.
 * @param ctx the parse tree
 */
fn exit_hier_ref(&mut self, _ctx: &Hier_refContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#measures_clause}.
 * @param ctx the parse tree
 */
fn enter_measures_clause(&mut self, _ctx: &Measures_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#measures_clause}.
 * @param ctx the parse tree
 */
fn exit_measures_clause(&mut self, _ctx: &Measures_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#av_measure}.
 * @param ctx the parse tree
 */
fn enter_av_measure(&mut self, _ctx: &Av_measureContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#av_measure}.
 * @param ctx the parse tree
 */
fn exit_av_measure(&mut self, _ctx: &Av_measureContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#base_meas_clause}.
 * @param ctx the parse tree
 */
fn enter_base_meas_clause(&mut self, _ctx: &Base_meas_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#base_meas_clause}.
 * @param ctx the parse tree
 */
fn exit_base_meas_clause(&mut self, _ctx: &Base_meas_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#meas_aggregate_clause}.
 * @param ctx the parse tree
 */
fn enter_meas_aggregate_clause(&mut self, _ctx: &Meas_aggregate_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#meas_aggregate_clause}.
 * @param ctx the parse tree
 */
fn exit_meas_aggregate_clause(&mut self, _ctx: &Meas_aggregate_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#calc_meas_clause}.
 * @param ctx the parse tree
 */
fn enter_calc_meas_clause(&mut self, _ctx: &Calc_meas_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#calc_meas_clause}.
 * @param ctx the parse tree
 */
fn exit_calc_meas_clause(&mut self, _ctx: &Calc_meas_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_measure_clause}.
 * @param ctx the parse tree
 */
fn enter_default_measure_clause(&mut self, _ctx: &Default_measure_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_measure_clause}.
 * @param ctx the parse tree
 */
fn exit_default_measure_clause(&mut self, _ctx: &Default_measure_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_aggregate_clause}.
 * @param ctx the parse tree
 */
fn enter_default_aggregate_clause(&mut self, _ctx: &Default_aggregate_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_aggregate_clause}.
 * @param ctx the parse tree
 */
fn exit_default_aggregate_clause(&mut self, _ctx: &Default_aggregate_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cache_clause}.
 * @param ctx the parse tree
 */
fn enter_cache_clause(&mut self, _ctx: &Cache_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cache_clause}.
 * @param ctx the parse tree
 */
fn exit_cache_clause(&mut self, _ctx: &Cache_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cache_specification}.
 * @param ctx the parse tree
 */
fn enter_cache_specification(&mut self, _ctx: &Cache_specificationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cache_specification}.
 * @param ctx the parse tree
 */
fn exit_cache_specification(&mut self, _ctx: &Cache_specificationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#levels_clause}.
 * @param ctx the parse tree
 */
fn enter_levels_clause(&mut self, _ctx: &Levels_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#levels_clause}.
 * @param ctx the parse tree
 */
fn exit_levels_clause(&mut self, _ctx: &Levels_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#level_specification}.
 * @param ctx the parse tree
 */
fn enter_level_specification(&mut self, _ctx: &Level_specificationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#level_specification}.
 * @param ctx the parse tree
 */
fn exit_level_specification(&mut self, _ctx: &Level_specificationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#level_group_type}.
 * @param ctx the parse tree
 */
fn enter_level_group_type(&mut self, _ctx: &Level_group_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#level_group_type}.
 * @param ctx the parse tree
 */
fn exit_level_group_type(&mut self, _ctx: &Level_group_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#fact_columns_clause}.
 * @param ctx the parse tree
 */
fn enter_fact_columns_clause(&mut self, _ctx: &Fact_columns_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#fact_columns_clause}.
 * @param ctx the parse tree
 */
fn exit_fact_columns_clause(&mut self, _ctx: &Fact_columns_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#qry_transform_clause}.
 * @param ctx the parse tree
 */
fn enter_qry_transform_clause(&mut self, _ctx: &Qry_transform_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#qry_transform_clause}.
 * @param ctx the parse tree
 */
fn exit_qry_transform_clause(&mut self, _ctx: &Qry_transform_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_attribute_dimension}.
 * @param ctx the parse tree
 */
fn enter_create_attribute_dimension(&mut self, _ctx: &Create_attribute_dimensionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_attribute_dimension}.
 * @param ctx the parse tree
 */
fn exit_create_attribute_dimension(&mut self, _ctx: &Create_attribute_dimensionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ad_using_clause}.
 * @param ctx the parse tree
 */
fn enter_ad_using_clause(&mut self, _ctx: &Ad_using_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ad_using_clause}.
 * @param ctx the parse tree
 */
fn exit_ad_using_clause(&mut self, _ctx: &Ad_using_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#source_clause}.
 * @param ctx the parse tree
 */
fn enter_source_clause(&mut self, _ctx: &Source_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#source_clause}.
 * @param ctx the parse tree
 */
fn exit_source_clause(&mut self, _ctx: &Source_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#join_path_clause}.
 * @param ctx the parse tree
 */
fn enter_join_path_clause(&mut self, _ctx: &Join_path_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#join_path_clause}.
 * @param ctx the parse tree
 */
fn exit_join_path_clause(&mut self, _ctx: &Join_path_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#join_condition}.
 * @param ctx the parse tree
 */
fn enter_join_condition(&mut self, _ctx: &Join_conditionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#join_condition}.
 * @param ctx the parse tree
 */
fn exit_join_condition(&mut self, _ctx: &Join_conditionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#join_condition_item}.
 * @param ctx the parse tree
 */
fn enter_join_condition_item(&mut self, _ctx: &Join_condition_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#join_condition_item}.
 * @param ctx the parse tree
 */
fn exit_join_condition_item(&mut self, _ctx: &Join_condition_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#attributes_clause}.
 * @param ctx the parse tree
 */
fn enter_attributes_clause(&mut self, _ctx: &Attributes_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#attributes_clause}.
 * @param ctx the parse tree
 */
fn exit_attributes_clause(&mut self, _ctx: &Attributes_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ad_attributes_clause}.
 * @param ctx the parse tree
 */
fn enter_ad_attributes_clause(&mut self, _ctx: &Ad_attributes_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ad_attributes_clause}.
 * @param ctx the parse tree
 */
fn exit_ad_attributes_clause(&mut self, _ctx: &Ad_attributes_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ad_level_clause}.
 * @param ctx the parse tree
 */
fn enter_ad_level_clause(&mut self, _ctx: &Ad_level_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ad_level_clause}.
 * @param ctx the parse tree
 */
fn exit_ad_level_clause(&mut self, _ctx: &Ad_level_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#key_clause}.
 * @param ctx the parse tree
 */
fn enter_key_clause(&mut self, _ctx: &Key_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#key_clause}.
 * @param ctx the parse tree
 */
fn exit_key_clause(&mut self, _ctx: &Key_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alternate_key_clause}.
 * @param ctx the parse tree
 */
fn enter_alternate_key_clause(&mut self, _ctx: &Alternate_key_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alternate_key_clause}.
 * @param ctx the parse tree
 */
fn exit_alternate_key_clause(&mut self, _ctx: &Alternate_key_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dim_order_clause}.
 * @param ctx the parse tree
 */
fn enter_dim_order_clause(&mut self, _ctx: &Dim_order_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dim_order_clause}.
 * @param ctx the parse tree
 */
fn exit_dim_order_clause(&mut self, _ctx: &Dim_order_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#all_clause}.
 * @param ctx the parse tree
 */
fn enter_all_clause(&mut self, _ctx: &All_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#all_clause}.
 * @param ctx the parse tree
 */
fn exit_all_clause(&mut self, _ctx: &All_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_audit_policy}.
 * @param ctx the parse tree
 */
fn enter_create_audit_policy(&mut self, _ctx: &Create_audit_policyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_audit_policy}.
 * @param ctx the parse tree
 */
fn exit_create_audit_policy(&mut self, _ctx: &Create_audit_policyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#privilege_audit_clause}.
 * @param ctx the parse tree
 */
fn enter_privilege_audit_clause(&mut self, _ctx: &Privilege_audit_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#privilege_audit_clause}.
 * @param ctx the parse tree
 */
fn exit_privilege_audit_clause(&mut self, _ctx: &Privilege_audit_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#action_audit_clause}.
 * @param ctx the parse tree
 */
fn enter_action_audit_clause(&mut self, _ctx: &Action_audit_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#action_audit_clause}.
 * @param ctx the parse tree
 */
fn exit_action_audit_clause(&mut self, _ctx: &Action_audit_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#system_actions}.
 * @param ctx the parse tree
 */
fn enter_system_actions(&mut self, _ctx: &System_actionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#system_actions}.
 * @param ctx the parse tree
 */
fn exit_system_actions(&mut self, _ctx: &System_actionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#standard_actions}.
 * @param ctx the parse tree
 */
fn enter_standard_actions(&mut self, _ctx: &Standard_actionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#standard_actions}.
 * @param ctx the parse tree
 */
fn exit_standard_actions(&mut self, _ctx: &Standard_actionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#actions_clause}.
 * @param ctx the parse tree
 */
fn enter_actions_clause(&mut self, _ctx: &Actions_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#actions_clause}.
 * @param ctx the parse tree
 */
fn exit_actions_clause(&mut self, _ctx: &Actions_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_action}.
 * @param ctx the parse tree
 */
fn enter_object_action(&mut self, _ctx: &Object_actionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_action}.
 * @param ctx the parse tree
 */
fn exit_object_action(&mut self, _ctx: &Object_actionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#system_action}.
 * @param ctx the parse tree
 */
fn enter_system_action(&mut self, _ctx: &System_actionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#system_action}.
 * @param ctx the parse tree
 */
fn exit_system_action(&mut self, _ctx: &System_actionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#component_actions}.
 * @param ctx the parse tree
 */
fn enter_component_actions(&mut self, _ctx: &Component_actionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#component_actions}.
 * @param ctx the parse tree
 */
fn exit_component_actions(&mut self, _ctx: &Component_actionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#component_action}.
 * @param ctx the parse tree
 */
fn enter_component_action(&mut self, _ctx: &Component_actionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#component_action}.
 * @param ctx the parse tree
 */
fn exit_component_action(&mut self, _ctx: &Component_actionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#role_audit_clause}.
 * @param ctx the parse tree
 */
fn enter_role_audit_clause(&mut self, _ctx: &Role_audit_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#role_audit_clause}.
 * @param ctx the parse tree
 */
fn exit_role_audit_clause(&mut self, _ctx: &Role_audit_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_controlfile}.
 * @param ctx the parse tree
 */
fn enter_create_controlfile(&mut self, _ctx: &Create_controlfileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_controlfile}.
 * @param ctx the parse tree
 */
fn exit_create_controlfile(&mut self, _ctx: &Create_controlfileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#controlfile_options}.
 * @param ctx the parse tree
 */
fn enter_controlfile_options(&mut self, _ctx: &Controlfile_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#controlfile_options}.
 * @param ctx the parse tree
 */
fn exit_controlfile_options(&mut self, _ctx: &Controlfile_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#logfile_clause}.
 * @param ctx the parse tree
 */
fn enter_logfile_clause(&mut self, _ctx: &Logfile_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#logfile_clause}.
 * @param ctx the parse tree
 */
fn exit_logfile_clause(&mut self, _ctx: &Logfile_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#character_set_clause}.
 * @param ctx the parse tree
 */
fn enter_character_set_clause(&mut self, _ctx: &Character_set_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#character_set_clause}.
 * @param ctx the parse tree
 */
fn exit_character_set_clause(&mut self, _ctx: &Character_set_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#file_specification}.
 * @param ctx the parse tree
 */
fn enter_file_specification(&mut self, _ctx: &File_specificationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#file_specification}.
 * @param ctx the parse tree
 */
fn exit_file_specification(&mut self, _ctx: &File_specificationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_diskgroup}.
 * @param ctx the parse tree
 */
fn enter_create_diskgroup(&mut self, _ctx: &Create_diskgroupContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_diskgroup}.
 * @param ctx the parse tree
 */
fn exit_create_diskgroup(&mut self, _ctx: &Create_diskgroupContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#qualified_disk_clause}.
 * @param ctx the parse tree
 */
fn enter_qualified_disk_clause(&mut self, _ctx: &Qualified_disk_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#qualified_disk_clause}.
 * @param ctx the parse tree
 */
fn exit_qualified_disk_clause(&mut self, _ctx: &Qualified_disk_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_edition}.
 * @param ctx the parse tree
 */
fn enter_create_edition(&mut self, _ctx: &Create_editionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_edition}.
 * @param ctx the parse tree
 */
fn exit_create_edition(&mut self, _ctx: &Create_editionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_flashback_archive}.
 * @param ctx the parse tree
 */
fn enter_create_flashback_archive(&mut self, _ctx: &Create_flashback_archiveContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_flashback_archive}.
 * @param ctx the parse tree
 */
fn exit_create_flashback_archive(&mut self, _ctx: &Create_flashback_archiveContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#flashback_archive_quota}.
 * @param ctx the parse tree
 */
fn enter_flashback_archive_quota(&mut self, _ctx: &Flashback_archive_quotaContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#flashback_archive_quota}.
 * @param ctx the parse tree
 */
fn exit_flashback_archive_quota(&mut self, _ctx: &Flashback_archive_quotaContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#flashback_archive_retention}.
 * @param ctx the parse tree
 */
fn enter_flashback_archive_retention(&mut self, _ctx: &Flashback_archive_retentionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#flashback_archive_retention}.
 * @param ctx the parse tree
 */
fn exit_flashback_archive_retention(&mut self, _ctx: &Flashback_archive_retentionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_hierarchy}.
 * @param ctx the parse tree
 */
fn enter_create_hierarchy(&mut self, _ctx: &Create_hierarchyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_hierarchy}.
 * @param ctx the parse tree
 */
fn exit_create_hierarchy(&mut self, _ctx: &Create_hierarchyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hier_using_clause}.
 * @param ctx the parse tree
 */
fn enter_hier_using_clause(&mut self, _ctx: &Hier_using_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hier_using_clause}.
 * @param ctx the parse tree
 */
fn exit_hier_using_clause(&mut self, _ctx: &Hier_using_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#level_hier_clause}.
 * @param ctx the parse tree
 */
fn enter_level_hier_clause(&mut self, _ctx: &Level_hier_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#level_hier_clause}.
 * @param ctx the parse tree
 */
fn exit_level_hier_clause(&mut self, _ctx: &Level_hier_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hier_attrs_clause}.
 * @param ctx the parse tree
 */
fn enter_hier_attrs_clause(&mut self, _ctx: &Hier_attrs_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hier_attrs_clause}.
 * @param ctx the parse tree
 */
fn exit_hier_attrs_clause(&mut self, _ctx: &Hier_attrs_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hier_attr_clause}.
 * @param ctx the parse tree
 */
fn enter_hier_attr_clause(&mut self, _ctx: &Hier_attr_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hier_attr_clause}.
 * @param ctx the parse tree
 */
fn exit_hier_attr_clause(&mut self, _ctx: &Hier_attr_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hier_attr_name}.
 * @param ctx the parse tree
 */
fn enter_hier_attr_name(&mut self, _ctx: &Hier_attr_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hier_attr_name}.
 * @param ctx the parse tree
 */
fn exit_hier_attr_name(&mut self, _ctx: &Hier_attr_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_index}.
 * @param ctx the parse tree
 */
fn enter_create_index(&mut self, _ctx: &Create_indexContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_index}.
 * @param ctx the parse tree
 */
fn exit_create_index(&mut self, _ctx: &Create_indexContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cluster_index_clause}.
 * @param ctx the parse tree
 */
fn enter_cluster_index_clause(&mut self, _ctx: &Cluster_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cluster_index_clause}.
 * @param ctx the parse tree
 */
fn exit_cluster_index_clause(&mut self, _ctx: &Cluster_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cluster_name}.
 * @param ctx the parse tree
 */
fn enter_cluster_name(&mut self, _ctx: &Cluster_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cluster_name}.
 * @param ctx the parse tree
 */
fn exit_cluster_name(&mut self, _ctx: &Cluster_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_index_clause}.
 * @param ctx the parse tree
 */
fn enter_table_index_clause(&mut self, _ctx: &Table_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_index_clause}.
 * @param ctx the parse tree
 */
fn exit_table_index_clause(&mut self, _ctx: &Table_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#bitmap_join_index_clause}.
 * @param ctx the parse tree
 */
fn enter_bitmap_join_index_clause(&mut self, _ctx: &Bitmap_join_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#bitmap_join_index_clause}.
 * @param ctx the parse tree
 */
fn exit_bitmap_join_index_clause(&mut self, _ctx: &Bitmap_join_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_expr}.
 * @param ctx the parse tree
 */
fn enter_index_expr(&mut self, _ctx: &Index_exprContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_expr}.
 * @param ctx the parse tree
 */
fn exit_index_expr(&mut self, _ctx: &Index_exprContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_properties}.
 * @param ctx the parse tree
 */
fn enter_index_properties(&mut self, _ctx: &Index_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_properties}.
 * @param ctx the parse tree
 */
fn exit_index_properties(&mut self, _ctx: &Index_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#domain_index_clause}.
 * @param ctx the parse tree
 */
fn enter_domain_index_clause(&mut self, _ctx: &Domain_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#domain_index_clause}.
 * @param ctx the parse tree
 */
fn exit_domain_index_clause(&mut self, _ctx: &Domain_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#local_domain_index_clause}.
 * @param ctx the parse tree
 */
fn enter_local_domain_index_clause(&mut self, _ctx: &Local_domain_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#local_domain_index_clause}.
 * @param ctx the parse tree
 */
fn exit_local_domain_index_clause(&mut self, _ctx: &Local_domain_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmlindex_clause}.
 * @param ctx the parse tree
 */
fn enter_xmlindex_clause(&mut self, _ctx: &Xmlindex_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmlindex_clause}.
 * @param ctx the parse tree
 */
fn exit_xmlindex_clause(&mut self, _ctx: &Xmlindex_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#local_xmlindex_clause}.
 * @param ctx the parse tree
 */
fn enter_local_xmlindex_clause(&mut self, _ctx: &Local_xmlindex_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#local_xmlindex_clause}.
 * @param ctx the parse tree
 */
fn exit_local_xmlindex_clause(&mut self, _ctx: &Local_xmlindex_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#global_partitioned_index}.
 * @param ctx the parse tree
 */
fn enter_global_partitioned_index(&mut self, _ctx: &Global_partitioned_indexContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#global_partitioned_index}.
 * @param ctx the parse tree
 */
fn exit_global_partitioned_index(&mut self, _ctx: &Global_partitioned_indexContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_partitioning_clause}.
 * @param ctx the parse tree
 */
fn enter_index_partitioning_clause(&mut self, _ctx: &Index_partitioning_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_partitioning_clause}.
 * @param ctx the parse tree
 */
fn exit_index_partitioning_clause(&mut self, _ctx: &Index_partitioning_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_partitioning_values_list}.
 * @param ctx the parse tree
 */
fn enter_index_partitioning_values_list(&mut self, _ctx: &Index_partitioning_values_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_partitioning_values_list}.
 * @param ctx the parse tree
 */
fn exit_index_partitioning_values_list(&mut self, _ctx: &Index_partitioning_values_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#local_partitioned_index}.
 * @param ctx the parse tree
 */
fn enter_local_partitioned_index(&mut self, _ctx: &Local_partitioned_indexContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#local_partitioned_index}.
 * @param ctx the parse tree
 */
fn exit_local_partitioned_index(&mut self, _ctx: &Local_partitioned_indexContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#on_range_partitioned_table}.
 * @param ctx the parse tree
 */
fn enter_on_range_partitioned_table(&mut self, _ctx: &On_range_partitioned_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#on_range_partitioned_table}.
 * @param ctx the parse tree
 */
fn exit_on_range_partitioned_table(&mut self, _ctx: &On_range_partitioned_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#on_list_partitioned_table}.
 * @param ctx the parse tree
 */
fn enter_on_list_partitioned_table(&mut self, _ctx: &On_list_partitioned_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#on_list_partitioned_table}.
 * @param ctx the parse tree
 */
fn exit_on_list_partitioned_table(&mut self, _ctx: &On_list_partitioned_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partitioned_table}.
 * @param ctx the parse tree
 */
fn enter_partitioned_table(&mut self, _ctx: &Partitioned_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partitioned_table}.
 * @param ctx the parse tree
 */
fn exit_partitioned_table(&mut self, _ctx: &Partitioned_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#on_hash_partitioned_table}.
 * @param ctx the parse tree
 */
fn enter_on_hash_partitioned_table(&mut self, _ctx: &On_hash_partitioned_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#on_hash_partitioned_table}.
 * @param ctx the parse tree
 */
fn exit_on_hash_partitioned_table(&mut self, _ctx: &On_hash_partitioned_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#on_hash_partitioned_clause}.
 * @param ctx the parse tree
 */
fn enter_on_hash_partitioned_clause(&mut self, _ctx: &On_hash_partitioned_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#on_hash_partitioned_clause}.
 * @param ctx the parse tree
 */
fn exit_on_hash_partitioned_clause(&mut self, _ctx: &On_hash_partitioned_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#on_comp_partitioned_table}.
 * @param ctx the parse tree
 */
fn enter_on_comp_partitioned_table(&mut self, _ctx: &On_comp_partitioned_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#on_comp_partitioned_table}.
 * @param ctx the parse tree
 */
fn exit_on_comp_partitioned_table(&mut self, _ctx: &On_comp_partitioned_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#on_comp_partitioned_clause}.
 * @param ctx the parse tree
 */
fn enter_on_comp_partitioned_clause(&mut self, _ctx: &On_comp_partitioned_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#on_comp_partitioned_clause}.
 * @param ctx the parse tree
 */
fn exit_on_comp_partitioned_clause(&mut self, _ctx: &On_comp_partitioned_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_subpartition_clause}.
 * @param ctx the parse tree
 */
fn enter_index_subpartition_clause(&mut self, _ctx: &Index_subpartition_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_subpartition_clause}.
 * @param ctx the parse tree
 */
fn exit_index_subpartition_clause(&mut self, _ctx: &Index_subpartition_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_subpartition_subclause}.
 * @param ctx the parse tree
 */
fn enter_index_subpartition_subclause(&mut self, _ctx: &Index_subpartition_subclauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_subpartition_subclause}.
 * @param ctx the parse tree
 */
fn exit_index_subpartition_subclause(&mut self, _ctx: &Index_subpartition_subclauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#odci_parameters}.
 * @param ctx the parse tree
 */
fn enter_odci_parameters(&mut self, _ctx: &Odci_parametersContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#odci_parameters}.
 * @param ctx the parse tree
 */
fn exit_odci_parameters(&mut self, _ctx: &Odci_parametersContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#indextype}.
 * @param ctx the parse tree
 */
fn enter_indextype(&mut self, _ctx: &IndextypeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#indextype}.
 * @param ctx the parse tree
 */
fn exit_indextype(&mut self, _ctx: &IndextypeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_index}.
 * @param ctx the parse tree
 */
fn enter_alter_index(&mut self, _ctx: &Alter_indexContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_index}.
 * @param ctx the parse tree
 */
fn exit_alter_index(&mut self, _ctx: &Alter_indexContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_index_ops_set1}.
 * @param ctx the parse tree
 */
fn enter_alter_index_ops_set1(&mut self, _ctx: &Alter_index_ops_set1Context<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_index_ops_set1}.
 * @param ctx the parse tree
 */
fn exit_alter_index_ops_set1(&mut self, _ctx: &Alter_index_ops_set1Context<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_index_ops_set2}.
 * @param ctx the parse tree
 */
fn enter_alter_index_ops_set2(&mut self, _ctx: &Alter_index_ops_set2Context<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_index_ops_set2}.
 * @param ctx the parse tree
 */
fn exit_alter_index_ops_set2(&mut self, _ctx: &Alter_index_ops_set2Context<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#visible_or_invisible}.
 * @param ctx the parse tree
 */
fn enter_visible_or_invisible(&mut self, _ctx: &Visible_or_invisibleContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#visible_or_invisible}.
 * @param ctx the parse tree
 */
fn exit_visible_or_invisible(&mut self, _ctx: &Visible_or_invisibleContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#monitoring_nomonitoring}.
 * @param ctx the parse tree
 */
fn enter_monitoring_nomonitoring(&mut self, _ctx: &Monitoring_nomonitoringContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#monitoring_nomonitoring}.
 * @param ctx the parse tree
 */
fn exit_monitoring_nomonitoring(&mut self, _ctx: &Monitoring_nomonitoringContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rebuild_clause}.
 * @param ctx the parse tree
 */
fn enter_rebuild_clause(&mut self, _ctx: &Rebuild_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rebuild_clause}.
 * @param ctx the parse tree
 */
fn exit_rebuild_clause(&mut self, _ctx: &Rebuild_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_index_partitioning}.
 * @param ctx the parse tree
 */
fn enter_alter_index_partitioning(&mut self, _ctx: &Alter_index_partitioningContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_index_partitioning}.
 * @param ctx the parse tree
 */
fn exit_alter_index_partitioning(&mut self, _ctx: &Alter_index_partitioningContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_index_default_attrs}.
 * @param ctx the parse tree
 */
fn enter_modify_index_default_attrs(&mut self, _ctx: &Modify_index_default_attrsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_index_default_attrs}.
 * @param ctx the parse tree
 */
fn exit_modify_index_default_attrs(&mut self, _ctx: &Modify_index_default_attrsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_hash_index_partition}.
 * @param ctx the parse tree
 */
fn enter_add_hash_index_partition(&mut self, _ctx: &Add_hash_index_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_hash_index_partition}.
 * @param ctx the parse tree
 */
fn exit_add_hash_index_partition(&mut self, _ctx: &Add_hash_index_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#coalesce_index_partition}.
 * @param ctx the parse tree
 */
fn enter_coalesce_index_partition(&mut self, _ctx: &Coalesce_index_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#coalesce_index_partition}.
 * @param ctx the parse tree
 */
fn exit_coalesce_index_partition(&mut self, _ctx: &Coalesce_index_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_index_partition}.
 * @param ctx the parse tree
 */
fn enter_modify_index_partition(&mut self, _ctx: &Modify_index_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_index_partition}.
 * @param ctx the parse tree
 */
fn exit_modify_index_partition(&mut self, _ctx: &Modify_index_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_index_partitions_ops}.
 * @param ctx the parse tree
 */
fn enter_modify_index_partitions_ops(&mut self, _ctx: &Modify_index_partitions_opsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_index_partitions_ops}.
 * @param ctx the parse tree
 */
fn exit_modify_index_partitions_ops(&mut self, _ctx: &Modify_index_partitions_opsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rename_index_partition}.
 * @param ctx the parse tree
 */
fn enter_rename_index_partition(&mut self, _ctx: &Rename_index_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rename_index_partition}.
 * @param ctx the parse tree
 */
fn exit_rename_index_partition(&mut self, _ctx: &Rename_index_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_index_partition}.
 * @param ctx the parse tree
 */
fn enter_drop_index_partition(&mut self, _ctx: &Drop_index_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_index_partition}.
 * @param ctx the parse tree
 */
fn exit_drop_index_partition(&mut self, _ctx: &Drop_index_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#split_index_partition}.
 * @param ctx the parse tree
 */
fn enter_split_index_partition(&mut self, _ctx: &Split_index_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#split_index_partition}.
 * @param ctx the parse tree
 */
fn exit_split_index_partition(&mut self, _ctx: &Split_index_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_partition_description}.
 * @param ctx the parse tree
 */
fn enter_index_partition_description(&mut self, _ctx: &Index_partition_descriptionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_partition_description}.
 * @param ctx the parse tree
 */
fn exit_index_partition_description(&mut self, _ctx: &Index_partition_descriptionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_index_subpartition}.
 * @param ctx the parse tree
 */
fn enter_modify_index_subpartition(&mut self, _ctx: &Modify_index_subpartitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_index_subpartition}.
 * @param ctx the parse tree
 */
fn exit_modify_index_subpartition(&mut self, _ctx: &Modify_index_subpartitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partition_name_old}.
 * @param ctx the parse tree
 */
fn enter_partition_name_old(&mut self, _ctx: &Partition_name_oldContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partition_name_old}.
 * @param ctx the parse tree
 */
fn exit_partition_name_old(&mut self, _ctx: &Partition_name_oldContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#new_partition_name}.
 * @param ctx the parse tree
 */
fn enter_new_partition_name(&mut self, _ctx: &New_partition_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#new_partition_name}.
 * @param ctx the parse tree
 */
fn exit_new_partition_name(&mut self, _ctx: &New_partition_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#new_index_name}.
 * @param ctx the parse tree
 */
fn enter_new_index_name(&mut self, _ctx: &New_index_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#new_index_name}.
 * @param ctx the parse tree
 */
fn exit_new_index_name(&mut self, _ctx: &New_index_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_inmemory_join_group}.
 * @param ctx the parse tree
 */
fn enter_alter_inmemory_join_group(&mut self, _ctx: &Alter_inmemory_join_groupContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_inmemory_join_group}.
 * @param ctx the parse tree
 */
fn exit_alter_inmemory_join_group(&mut self, _ctx: &Alter_inmemory_join_groupContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_user}.
 * @param ctx the parse tree
 */
fn enter_create_user(&mut self, _ctx: &Create_userContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_user}.
 * @param ctx the parse tree
 */
fn exit_create_user(&mut self, _ctx: &Create_userContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_user}.
 * @param ctx the parse tree
 */
fn enter_alter_user(&mut self, _ctx: &Alter_userContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_user}.
 * @param ctx the parse tree
 */
fn exit_alter_user(&mut self, _ctx: &Alter_userContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_user}.
 * @param ctx the parse tree
 */
fn enter_drop_user(&mut self, _ctx: &Drop_userContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_user}.
 * @param ctx the parse tree
 */
fn exit_drop_user(&mut self, _ctx: &Drop_userContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_identified_by}.
 * @param ctx the parse tree
 */
fn enter_alter_identified_by(&mut self, _ctx: &Alter_identified_byContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_identified_by}.
 * @param ctx the parse tree
 */
fn exit_alter_identified_by(&mut self, _ctx: &Alter_identified_byContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#identified_by}.
 * @param ctx the parse tree
 */
fn enter_identified_by(&mut self, _ctx: &Identified_byContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#identified_by}.
 * @param ctx the parse tree
 */
fn exit_identified_by(&mut self, _ctx: &Identified_byContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#identified_other_clause}.
 * @param ctx the parse tree
 */
fn enter_identified_other_clause(&mut self, _ctx: &Identified_other_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#identified_other_clause}.
 * @param ctx the parse tree
 */
fn exit_identified_other_clause(&mut self, _ctx: &Identified_other_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#user_tablespace_clause}.
 * @param ctx the parse tree
 */
fn enter_user_tablespace_clause(&mut self, _ctx: &User_tablespace_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#user_tablespace_clause}.
 * @param ctx the parse tree
 */
fn exit_user_tablespace_clause(&mut self, _ctx: &User_tablespace_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#quota_clause}.
 * @param ctx the parse tree
 */
fn enter_quota_clause(&mut self, _ctx: &Quota_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#quota_clause}.
 * @param ctx the parse tree
 */
fn exit_quota_clause(&mut self, _ctx: &Quota_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#profile_clause}.
 * @param ctx the parse tree
 */
fn enter_profile_clause(&mut self, _ctx: &Profile_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#profile_clause}.
 * @param ctx the parse tree
 */
fn exit_profile_clause(&mut self, _ctx: &Profile_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#role_clause}.
 * @param ctx the parse tree
 */
fn enter_role_clause(&mut self, _ctx: &Role_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#role_clause}.
 * @param ctx the parse tree
 */
fn exit_role_clause(&mut self, _ctx: &Role_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#user_default_role_clause}.
 * @param ctx the parse tree
 */
fn enter_user_default_role_clause(&mut self, _ctx: &User_default_role_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#user_default_role_clause}.
 * @param ctx the parse tree
 */
fn exit_user_default_role_clause(&mut self, _ctx: &User_default_role_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#password_expire_clause}.
 * @param ctx the parse tree
 */
fn enter_password_expire_clause(&mut self, _ctx: &Password_expire_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#password_expire_clause}.
 * @param ctx the parse tree
 */
fn exit_password_expire_clause(&mut self, _ctx: &Password_expire_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#user_lock_clause}.
 * @param ctx the parse tree
 */
fn enter_user_lock_clause(&mut self, _ctx: &User_lock_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#user_lock_clause}.
 * @param ctx the parse tree
 */
fn exit_user_lock_clause(&mut self, _ctx: &User_lock_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#user_editions_clause}.
 * @param ctx the parse tree
 */
fn enter_user_editions_clause(&mut self, _ctx: &User_editions_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#user_editions_clause}.
 * @param ctx the parse tree
 */
fn exit_user_editions_clause(&mut self, _ctx: &User_editions_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_user_editions_clause}.
 * @param ctx the parse tree
 */
fn enter_alter_user_editions_clause(&mut self, _ctx: &Alter_user_editions_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_user_editions_clause}.
 * @param ctx the parse tree
 */
fn exit_alter_user_editions_clause(&mut self, _ctx: &Alter_user_editions_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#proxy_clause}.
 * @param ctx the parse tree
 */
fn enter_proxy_clause(&mut self, _ctx: &Proxy_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#proxy_clause}.
 * @param ctx the parse tree
 */
fn exit_proxy_clause(&mut self, _ctx: &Proxy_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#container_names}.
 * @param ctx the parse tree
 */
fn enter_container_names(&mut self, _ctx: &Container_namesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#container_names}.
 * @param ctx the parse tree
 */
fn exit_container_names(&mut self, _ctx: &Container_namesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#set_container_data}.
 * @param ctx the parse tree
 */
fn enter_set_container_data(&mut self, _ctx: &Set_container_dataContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#set_container_data}.
 * @param ctx the parse tree
 */
fn exit_set_container_data(&mut self, _ctx: &Set_container_dataContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_rem_container_data}.
 * @param ctx the parse tree
 */
fn enter_add_rem_container_data(&mut self, _ctx: &Add_rem_container_dataContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_rem_container_data}.
 * @param ctx the parse tree
 */
fn exit_add_rem_container_data(&mut self, _ctx: &Add_rem_container_dataContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#container_data_clause}.
 * @param ctx the parse tree
 */
fn enter_container_data_clause(&mut self, _ctx: &Container_data_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#container_data_clause}.
 * @param ctx the parse tree
 */
fn exit_container_data_clause(&mut self, _ctx: &Container_data_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#administer_key_management}.
 * @param ctx the parse tree
 */
fn enter_administer_key_management(&mut self, _ctx: &Administer_key_managementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#administer_key_management}.
 * @param ctx the parse tree
 */
fn exit_administer_key_management(&mut self, _ctx: &Administer_key_managementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#keystore_management_clauses}.
 * @param ctx the parse tree
 */
fn enter_keystore_management_clauses(&mut self, _ctx: &Keystore_management_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#keystore_management_clauses}.
 * @param ctx the parse tree
 */
fn exit_keystore_management_clauses(&mut self, _ctx: &Keystore_management_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_keystore}.
 * @param ctx the parse tree
 */
fn enter_create_keystore(&mut self, _ctx: &Create_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_keystore}.
 * @param ctx the parse tree
 */
fn exit_create_keystore(&mut self, _ctx: &Create_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#open_keystore}.
 * @param ctx the parse tree
 */
fn enter_open_keystore(&mut self, _ctx: &Open_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#open_keystore}.
 * @param ctx the parse tree
 */
fn exit_open_keystore(&mut self, _ctx: &Open_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#force_keystore}.
 * @param ctx the parse tree
 */
fn enter_force_keystore(&mut self, _ctx: &Force_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#force_keystore}.
 * @param ctx the parse tree
 */
fn exit_force_keystore(&mut self, _ctx: &Force_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#close_keystore}.
 * @param ctx the parse tree
 */
fn enter_close_keystore(&mut self, _ctx: &Close_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#close_keystore}.
 * @param ctx the parse tree
 */
fn exit_close_keystore(&mut self, _ctx: &Close_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#backup_keystore}.
 * @param ctx the parse tree
 */
fn enter_backup_keystore(&mut self, _ctx: &Backup_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#backup_keystore}.
 * @param ctx the parse tree
 */
fn exit_backup_keystore(&mut self, _ctx: &Backup_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_keystore_password}.
 * @param ctx the parse tree
 */
fn enter_alter_keystore_password(&mut self, _ctx: &Alter_keystore_passwordContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_keystore_password}.
 * @param ctx the parse tree
 */
fn exit_alter_keystore_password(&mut self, _ctx: &Alter_keystore_passwordContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#merge_into_new_keystore}.
 * @param ctx the parse tree
 */
fn enter_merge_into_new_keystore(&mut self, _ctx: &Merge_into_new_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#merge_into_new_keystore}.
 * @param ctx the parse tree
 */
fn exit_merge_into_new_keystore(&mut self, _ctx: &Merge_into_new_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#merge_into_existing_keystore}.
 * @param ctx the parse tree
 */
fn enter_merge_into_existing_keystore(&mut self, _ctx: &Merge_into_existing_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#merge_into_existing_keystore}.
 * @param ctx the parse tree
 */
fn exit_merge_into_existing_keystore(&mut self, _ctx: &Merge_into_existing_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#isolate_keystore}.
 * @param ctx the parse tree
 */
fn enter_isolate_keystore(&mut self, _ctx: &Isolate_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#isolate_keystore}.
 * @param ctx the parse tree
 */
fn exit_isolate_keystore(&mut self, _ctx: &Isolate_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unite_keystore}.
 * @param ctx the parse tree
 */
fn enter_unite_keystore(&mut self, _ctx: &Unite_keystoreContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unite_keystore}.
 * @param ctx the parse tree
 */
fn exit_unite_keystore(&mut self, _ctx: &Unite_keystoreContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#key_management_clauses}.
 * @param ctx the parse tree
 */
fn enter_key_management_clauses(&mut self, _ctx: &Key_management_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#key_management_clauses}.
 * @param ctx the parse tree
 */
fn exit_key_management_clauses(&mut self, _ctx: &Key_management_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#set_key}.
 * @param ctx the parse tree
 */
fn enter_set_key(&mut self, _ctx: &Set_keyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#set_key}.
 * @param ctx the parse tree
 */
fn exit_set_key(&mut self, _ctx: &Set_keyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_key}.
 * @param ctx the parse tree
 */
fn enter_create_key(&mut self, _ctx: &Create_keyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_key}.
 * @param ctx the parse tree
 */
fn exit_create_key(&mut self, _ctx: &Create_keyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#mkid}.
 * @param ctx the parse tree
 */
fn enter_mkid(&mut self, _ctx: &MkidContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#mkid}.
 * @param ctx the parse tree
 */
fn exit_mkid(&mut self, _ctx: &MkidContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#mk}.
 * @param ctx the parse tree
 */
fn enter_mk(&mut self, _ctx: &MkContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#mk}.
 * @param ctx the parse tree
 */
fn exit_mk(&mut self, _ctx: &MkContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#use_key}.
 * @param ctx the parse tree
 */
fn enter_use_key(&mut self, _ctx: &Use_keyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#use_key}.
 * @param ctx the parse tree
 */
fn exit_use_key(&mut self, _ctx: &Use_keyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#set_key_tag}.
 * @param ctx the parse tree
 */
fn enter_set_key_tag(&mut self, _ctx: &Set_key_tagContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#set_key_tag}.
 * @param ctx the parse tree
 */
fn exit_set_key_tag(&mut self, _ctx: &Set_key_tagContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#export_keys}.
 * @param ctx the parse tree
 */
fn enter_export_keys(&mut self, _ctx: &Export_keysContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#export_keys}.
 * @param ctx the parse tree
 */
fn exit_export_keys(&mut self, _ctx: &Export_keysContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#import_keys}.
 * @param ctx the parse tree
 */
fn enter_import_keys(&mut self, _ctx: &Import_keysContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#import_keys}.
 * @param ctx the parse tree
 */
fn exit_import_keys(&mut self, _ctx: &Import_keysContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#migrate_keys}.
 * @param ctx the parse tree
 */
fn enter_migrate_keys(&mut self, _ctx: &Migrate_keysContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#migrate_keys}.
 * @param ctx the parse tree
 */
fn exit_migrate_keys(&mut self, _ctx: &Migrate_keysContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#reverse_migrate_keys}.
 * @param ctx the parse tree
 */
fn enter_reverse_migrate_keys(&mut self, _ctx: &Reverse_migrate_keysContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#reverse_migrate_keys}.
 * @param ctx the parse tree
 */
fn exit_reverse_migrate_keys(&mut self, _ctx: &Reverse_migrate_keysContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#move_keys}.
 * @param ctx the parse tree
 */
fn enter_move_keys(&mut self, _ctx: &Move_keysContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#move_keys}.
 * @param ctx the parse tree
 */
fn exit_move_keys(&mut self, _ctx: &Move_keysContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#identified_by_store}.
 * @param ctx the parse tree
 */
fn enter_identified_by_store(&mut self, _ctx: &Identified_by_storeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#identified_by_store}.
 * @param ctx the parse tree
 */
fn exit_identified_by_store(&mut self, _ctx: &Identified_by_storeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#using_algorithm_clause}.
 * @param ctx the parse tree
 */
fn enter_using_algorithm_clause(&mut self, _ctx: &Using_algorithm_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#using_algorithm_clause}.
 * @param ctx the parse tree
 */
fn exit_using_algorithm_clause(&mut self, _ctx: &Using_algorithm_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#using_tag_clause}.
 * @param ctx the parse tree
 */
fn enter_using_tag_clause(&mut self, _ctx: &Using_tag_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#using_tag_clause}.
 * @param ctx the parse tree
 */
fn exit_using_tag_clause(&mut self, _ctx: &Using_tag_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#secret_management_clauses}.
 * @param ctx the parse tree
 */
fn enter_secret_management_clauses(&mut self, _ctx: &Secret_management_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#secret_management_clauses}.
 * @param ctx the parse tree
 */
fn exit_secret_management_clauses(&mut self, _ctx: &Secret_management_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_update_secret}.
 * @param ctx the parse tree
 */
fn enter_add_update_secret(&mut self, _ctx: &Add_update_secretContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_update_secret}.
 * @param ctx the parse tree
 */
fn exit_add_update_secret(&mut self, _ctx: &Add_update_secretContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#delete_secret}.
 * @param ctx the parse tree
 */
fn enter_delete_secret(&mut self, _ctx: &Delete_secretContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#delete_secret}.
 * @param ctx the parse tree
 */
fn exit_delete_secret(&mut self, _ctx: &Delete_secretContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_update_secret_seps}.
 * @param ctx the parse tree
 */
fn enter_add_update_secret_seps(&mut self, _ctx: &Add_update_secret_sepsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_update_secret_seps}.
 * @param ctx the parse tree
 */
fn exit_add_update_secret_seps(&mut self, _ctx: &Add_update_secret_sepsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#delete_secret_seps}.
 * @param ctx the parse tree
 */
fn enter_delete_secret_seps(&mut self, _ctx: &Delete_secret_sepsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#delete_secret_seps}.
 * @param ctx the parse tree
 */
fn exit_delete_secret_seps(&mut self, _ctx: &Delete_secret_sepsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#zero_downtime_software_patching_clauses}.
 * @param ctx the parse tree
 */
fn enter_zero_downtime_software_patching_clauses(&mut self, _ctx: &Zero_downtime_software_patching_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#zero_downtime_software_patching_clauses}.
 * @param ctx the parse tree
 */
fn exit_zero_downtime_software_patching_clauses(&mut self, _ctx: &Zero_downtime_software_patching_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#with_backup_clause}.
 * @param ctx the parse tree
 */
fn enter_with_backup_clause(&mut self, _ctx: &With_backup_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#with_backup_clause}.
 * @param ctx the parse tree
 */
fn exit_with_backup_clause(&mut self, _ctx: &With_backup_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#identified_by_password_clause}.
 * @param ctx the parse tree
 */
fn enter_identified_by_password_clause(&mut self, _ctx: &Identified_by_password_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#identified_by_password_clause}.
 * @param ctx the parse tree
 */
fn exit_identified_by_password_clause(&mut self, _ctx: &Identified_by_password_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#keystore_password}.
 * @param ctx the parse tree
 */
fn enter_keystore_password(&mut self, _ctx: &Keystore_passwordContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#keystore_password}.
 * @param ctx the parse tree
 */
fn exit_keystore_password(&mut self, _ctx: &Keystore_passwordContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#path}.
 * @param ctx the parse tree
 */
fn enter_path(&mut self, _ctx: &PathContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#path}.
 * @param ctx the parse tree
 */
fn exit_path(&mut self, _ctx: &PathContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#secret}.
 * @param ctx the parse tree
 */
fn enter_secret(&mut self, _ctx: &SecretContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#secret}.
 * @param ctx the parse tree
 */
fn exit_secret(&mut self, _ctx: &SecretContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#analyze}.
 * @param ctx the parse tree
 */
fn enter_analyze(&mut self, _ctx: &AnalyzeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#analyze}.
 * @param ctx the parse tree
 */
fn exit_analyze(&mut self, _ctx: &AnalyzeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partition_extention_clause}.
 * @param ctx the parse tree
 */
fn enter_partition_extention_clause(&mut self, _ctx: &Partition_extention_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partition_extention_clause}.
 * @param ctx the parse tree
 */
fn exit_partition_extention_clause(&mut self, _ctx: &Partition_extention_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#validation_clauses}.
 * @param ctx the parse tree
 */
fn enter_validation_clauses(&mut self, _ctx: &Validation_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#validation_clauses}.
 * @param ctx the parse tree
 */
fn exit_validation_clauses(&mut self, _ctx: &Validation_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#compute_clauses}.
 * @param ctx the parse tree
 */
fn enter_compute_clauses(&mut self, _ctx: &Compute_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#compute_clauses}.
 * @param ctx the parse tree
 */
fn exit_compute_clauses(&mut self, _ctx: &Compute_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#for_clause}.
 * @param ctx the parse tree
 */
fn enter_for_clause(&mut self, _ctx: &For_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#for_clause}.
 * @param ctx the parse tree
 */
fn exit_for_clause(&mut self, _ctx: &For_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#online_or_offline}.
 * @param ctx the parse tree
 */
fn enter_online_or_offline(&mut self, _ctx: &Online_or_offlineContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#online_or_offline}.
 * @param ctx the parse tree
 */
fn exit_online_or_offline(&mut self, _ctx: &Online_or_offlineContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#into_clause1}.
 * @param ctx the parse tree
 */
fn enter_into_clause1(&mut self, _ctx: &Into_clause1Context<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#into_clause1}.
 * @param ctx the parse tree
 */
fn exit_into_clause1(&mut self, _ctx: &Into_clause1Context<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partition_key_value}.
 * @param ctx the parse tree
 */
fn enter_partition_key_value(&mut self, _ctx: &Partition_key_valueContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partition_key_value}.
 * @param ctx the parse tree
 */
fn exit_partition_key_value(&mut self, _ctx: &Partition_key_valueContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subpartition_key_value}.
 * @param ctx the parse tree
 */
fn enter_subpartition_key_value(&mut self, _ctx: &Subpartition_key_valueContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subpartition_key_value}.
 * @param ctx the parse tree
 */
fn exit_subpartition_key_value(&mut self, _ctx: &Subpartition_key_valueContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#associate_statistics}.
 * @param ctx the parse tree
 */
fn enter_associate_statistics(&mut self, _ctx: &Associate_statisticsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#associate_statistics}.
 * @param ctx the parse tree
 */
fn exit_associate_statistics(&mut self, _ctx: &Associate_statisticsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_association}.
 * @param ctx the parse tree
 */
fn enter_column_association(&mut self, _ctx: &Column_associationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_association}.
 * @param ctx the parse tree
 */
fn exit_column_association(&mut self, _ctx: &Column_associationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#function_association}.
 * @param ctx the parse tree
 */
fn enter_function_association(&mut self, _ctx: &Function_associationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#function_association}.
 * @param ctx the parse tree
 */
fn exit_function_association(&mut self, _ctx: &Function_associationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#indextype_name}.
 * @param ctx the parse tree
 */
fn enter_indextype_name(&mut self, _ctx: &Indextype_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#indextype_name}.
 * @param ctx the parse tree
 */
fn exit_indextype_name(&mut self, _ctx: &Indextype_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#using_statistics_type}.
 * @param ctx the parse tree
 */
fn enter_using_statistics_type(&mut self, _ctx: &Using_statistics_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#using_statistics_type}.
 * @param ctx the parse tree
 */
fn exit_using_statistics_type(&mut self, _ctx: &Using_statistics_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#statistics_type_name}.
 * @param ctx the parse tree
 */
fn enter_statistics_type_name(&mut self, _ctx: &Statistics_type_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#statistics_type_name}.
 * @param ctx the parse tree
 */
fn exit_statistics_type_name(&mut self, _ctx: &Statistics_type_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_cost_clause}.
 * @param ctx the parse tree
 */
fn enter_default_cost_clause(&mut self, _ctx: &Default_cost_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_cost_clause}.
 * @param ctx the parse tree
 */
fn exit_default_cost_clause(&mut self, _ctx: &Default_cost_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cpu_cost}.
 * @param ctx the parse tree
 */
fn enter_cpu_cost(&mut self, _ctx: &Cpu_costContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cpu_cost}.
 * @param ctx the parse tree
 */
fn exit_cpu_cost(&mut self, _ctx: &Cpu_costContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#io_cost}.
 * @param ctx the parse tree
 */
fn enter_io_cost(&mut self, _ctx: &Io_costContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#io_cost}.
 * @param ctx the parse tree
 */
fn exit_io_cost(&mut self, _ctx: &Io_costContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#network_cost}.
 * @param ctx the parse tree
 */
fn enter_network_cost(&mut self, _ctx: &Network_costContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#network_cost}.
 * @param ctx the parse tree
 */
fn exit_network_cost(&mut self, _ctx: &Network_costContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_selectivity_clause}.
 * @param ctx the parse tree
 */
fn enter_default_selectivity_clause(&mut self, _ctx: &Default_selectivity_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_selectivity_clause}.
 * @param ctx the parse tree
 */
fn exit_default_selectivity_clause(&mut self, _ctx: &Default_selectivity_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_selectivity}.
 * @param ctx the parse tree
 */
fn enter_default_selectivity(&mut self, _ctx: &Default_selectivityContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_selectivity}.
 * @param ctx the parse tree
 */
fn exit_default_selectivity(&mut self, _ctx: &Default_selectivityContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#storage_table_clause}.
 * @param ctx the parse tree
 */
fn enter_storage_table_clause(&mut self, _ctx: &Storage_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#storage_table_clause}.
 * @param ctx the parse tree
 */
fn exit_storage_table_clause(&mut self, _ctx: &Storage_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unified_auditing}.
 * @param ctx the parse tree
 */
fn enter_unified_auditing(&mut self, _ctx: &Unified_auditingContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unified_auditing}.
 * @param ctx the parse tree
 */
fn exit_unified_auditing(&mut self, _ctx: &Unified_auditingContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#policy_name}.
 * @param ctx the parse tree
 */
fn enter_policy_name(&mut self, _ctx: &Policy_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#policy_name}.
 * @param ctx the parse tree
 */
fn exit_policy_name(&mut self, _ctx: &Policy_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#audit_traditional}.
 * @param ctx the parse tree
 */
fn enter_audit_traditional(&mut self, _ctx: &Audit_traditionalContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#audit_traditional}.
 * @param ctx the parse tree
 */
fn exit_audit_traditional(&mut self, _ctx: &Audit_traditionalContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#audit_direct_path}.
 * @param ctx the parse tree
 */
fn enter_audit_direct_path(&mut self, _ctx: &Audit_direct_pathContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#audit_direct_path}.
 * @param ctx the parse tree
 */
fn exit_audit_direct_path(&mut self, _ctx: &Audit_direct_pathContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#audit_container_clause}.
 * @param ctx the parse tree
 */
fn enter_audit_container_clause(&mut self, _ctx: &Audit_container_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#audit_container_clause}.
 * @param ctx the parse tree
 */
fn exit_audit_container_clause(&mut self, _ctx: &Audit_container_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#audit_operation_clause}.
 * @param ctx the parse tree
 */
fn enter_audit_operation_clause(&mut self, _ctx: &Audit_operation_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#audit_operation_clause}.
 * @param ctx the parse tree
 */
fn exit_audit_operation_clause(&mut self, _ctx: &Audit_operation_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#auditing_by_clause}.
 * @param ctx the parse tree
 */
fn enter_auditing_by_clause(&mut self, _ctx: &Auditing_by_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#auditing_by_clause}.
 * @param ctx the parse tree
 */
fn exit_auditing_by_clause(&mut self, _ctx: &Auditing_by_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#audit_user}.
 * @param ctx the parse tree
 */
fn enter_audit_user(&mut self, _ctx: &Audit_userContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#audit_user}.
 * @param ctx the parse tree
 */
fn exit_audit_user(&mut self, _ctx: &Audit_userContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#audit_schema_object_clause}.
 * @param ctx the parse tree
 */
fn enter_audit_schema_object_clause(&mut self, _ctx: &Audit_schema_object_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#audit_schema_object_clause}.
 * @param ctx the parse tree
 */
fn exit_audit_schema_object_clause(&mut self, _ctx: &Audit_schema_object_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sql_operation}.
 * @param ctx the parse tree
 */
fn enter_sql_operation(&mut self, _ctx: &Sql_operationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sql_operation}.
 * @param ctx the parse tree
 */
fn exit_sql_operation(&mut self, _ctx: &Sql_operationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#auditing_on_clause}.
 * @param ctx the parse tree
 */
fn enter_auditing_on_clause(&mut self, _ctx: &Auditing_on_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#auditing_on_clause}.
 * @param ctx the parse tree
 */
fn exit_auditing_on_clause(&mut self, _ctx: &Auditing_on_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_name}.
 * @param ctx the parse tree
 */
fn enter_model_name(&mut self, _ctx: &Model_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_name}.
 * @param ctx the parse tree
 */
fn exit_model_name(&mut self, _ctx: &Model_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_name}.
 * @param ctx the parse tree
 */
fn enter_object_name(&mut self, _ctx: &Object_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_name}.
 * @param ctx the parse tree
 */
fn exit_object_name(&mut self, _ctx: &Object_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#profile_name}.
 * @param ctx the parse tree
 */
fn enter_profile_name(&mut self, _ctx: &Profile_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#profile_name}.
 * @param ctx the parse tree
 */
fn exit_profile_name(&mut self, _ctx: &Profile_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sql_statement_shortcut}.
 * @param ctx the parse tree
 */
fn enter_sql_statement_shortcut(&mut self, _ctx: &Sql_statement_shortcutContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sql_statement_shortcut}.
 * @param ctx the parse tree
 */
fn exit_sql_statement_shortcut(&mut self, _ctx: &Sql_statement_shortcutContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_index}.
 * @param ctx the parse tree
 */
fn enter_drop_index(&mut self, _ctx: &Drop_indexContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_index}.
 * @param ctx the parse tree
 */
fn exit_drop_index(&mut self, _ctx: &Drop_indexContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#disassociate_statistics}.
 * @param ctx the parse tree
 */
fn enter_disassociate_statistics(&mut self, _ctx: &Disassociate_statisticsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#disassociate_statistics}.
 * @param ctx the parse tree
 */
fn exit_disassociate_statistics(&mut self, _ctx: &Disassociate_statisticsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_indextype}.
 * @param ctx the parse tree
 */
fn enter_drop_indextype(&mut self, _ctx: &Drop_indextypeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_indextype}.
 * @param ctx the parse tree
 */
fn exit_drop_indextype(&mut self, _ctx: &Drop_indextypeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_inmemory_join_group}.
 * @param ctx the parse tree
 */
fn enter_drop_inmemory_join_group(&mut self, _ctx: &Drop_inmemory_join_groupContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_inmemory_join_group}.
 * @param ctx the parse tree
 */
fn exit_drop_inmemory_join_group(&mut self, _ctx: &Drop_inmemory_join_groupContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#flashback_table}.
 * @param ctx the parse tree
 */
fn enter_flashback_table(&mut self, _ctx: &Flashback_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#flashback_table}.
 * @param ctx the parse tree
 */
fn exit_flashback_table(&mut self, _ctx: &Flashback_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#restore_point}.
 * @param ctx the parse tree
 */
fn enter_restore_point(&mut self, _ctx: &Restore_pointContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#restore_point}.
 * @param ctx the parse tree
 */
fn exit_restore_point(&mut self, _ctx: &Restore_pointContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#purge_statement}.
 * @param ctx the parse tree
 */
fn enter_purge_statement(&mut self, _ctx: &Purge_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#purge_statement}.
 * @param ctx the parse tree
 */
fn exit_purge_statement(&mut self, _ctx: &Purge_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#noaudit_statement}.
 * @param ctx the parse tree
 */
fn enter_noaudit_statement(&mut self, _ctx: &Noaudit_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#noaudit_statement}.
 * @param ctx the parse tree
 */
fn exit_noaudit_statement(&mut self, _ctx: &Noaudit_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rename_object}.
 * @param ctx the parse tree
 */
fn enter_rename_object(&mut self, _ctx: &Rename_objectContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rename_object}.
 * @param ctx the parse tree
 */
fn exit_rename_object(&mut self, _ctx: &Rename_objectContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#grant_statement}.
 * @param ctx the parse tree
 */
fn enter_grant_statement(&mut self, _ctx: &Grant_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#grant_statement}.
 * @param ctx the parse tree
 */
fn exit_grant_statement(&mut self, _ctx: &Grant_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#container_clause}.
 * @param ctx the parse tree
 */
fn enter_container_clause(&mut self, _ctx: &Container_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#container_clause}.
 * @param ctx the parse tree
 */
fn exit_container_clause(&mut self, _ctx: &Container_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#revoke_statement}.
 * @param ctx the parse tree
 */
fn enter_revoke_statement(&mut self, _ctx: &Revoke_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#revoke_statement}.
 * @param ctx the parse tree
 */
fn exit_revoke_statement(&mut self, _ctx: &Revoke_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#revoke_system_privilege}.
 * @param ctx the parse tree
 */
fn enter_revoke_system_privilege(&mut self, _ctx: &Revoke_system_privilegeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#revoke_system_privilege}.
 * @param ctx the parse tree
 */
fn exit_revoke_system_privilege(&mut self, _ctx: &Revoke_system_privilegeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#revokee_clause}.
 * @param ctx the parse tree
 */
fn enter_revokee_clause(&mut self, _ctx: &Revokee_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#revokee_clause}.
 * @param ctx the parse tree
 */
fn exit_revokee_clause(&mut self, _ctx: &Revokee_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#revoke_object_privileges}.
 * @param ctx the parse tree
 */
fn enter_revoke_object_privileges(&mut self, _ctx: &Revoke_object_privilegesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#revoke_object_privileges}.
 * @param ctx the parse tree
 */
fn exit_revoke_object_privileges(&mut self, _ctx: &Revoke_object_privilegesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#on_object_clause}.
 * @param ctx the parse tree
 */
fn enter_on_object_clause(&mut self, _ctx: &On_object_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#on_object_clause}.
 * @param ctx the parse tree
 */
fn exit_on_object_clause(&mut self, _ctx: &On_object_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#revoke_roles_from_programs}.
 * @param ctx the parse tree
 */
fn enter_revoke_roles_from_programs(&mut self, _ctx: &Revoke_roles_from_programsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#revoke_roles_from_programs}.
 * @param ctx the parse tree
 */
fn exit_revoke_roles_from_programs(&mut self, _ctx: &Revoke_roles_from_programsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#program_unit}.
 * @param ctx the parse tree
 */
fn enter_program_unit(&mut self, _ctx: &Program_unitContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#program_unit}.
 * @param ctx the parse tree
 */
fn exit_program_unit(&mut self, _ctx: &Program_unitContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_dimension}.
 * @param ctx the parse tree
 */
fn enter_create_dimension(&mut self, _ctx: &Create_dimensionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_dimension}.
 * @param ctx the parse tree
 */
fn exit_create_dimension(&mut self, _ctx: &Create_dimensionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_directory}.
 * @param ctx the parse tree
 */
fn enter_create_directory(&mut self, _ctx: &Create_directoryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_directory}.
 * @param ctx the parse tree
 */
fn exit_create_directory(&mut self, _ctx: &Create_directoryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#directory_name}.
 * @param ctx the parse tree
 */
fn enter_directory_name(&mut self, _ctx: &Directory_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#directory_name}.
 * @param ctx the parse tree
 */
fn exit_directory_name(&mut self, _ctx: &Directory_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#directory_path}.
 * @param ctx the parse tree
 */
fn enter_directory_path(&mut self, _ctx: &Directory_pathContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#directory_path}.
 * @param ctx the parse tree
 */
fn exit_directory_path(&mut self, _ctx: &Directory_pathContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_inmemory_join_group}.
 * @param ctx the parse tree
 */
fn enter_create_inmemory_join_group(&mut self, _ctx: &Create_inmemory_join_groupContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_inmemory_join_group}.
 * @param ctx the parse tree
 */
fn exit_create_inmemory_join_group(&mut self, _ctx: &Create_inmemory_join_groupContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_hierarchy}.
 * @param ctx the parse tree
 */
fn enter_drop_hierarchy(&mut self, _ctx: &Drop_hierarchyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_hierarchy}.
 * @param ctx the parse tree
 */
fn exit_drop_hierarchy(&mut self, _ctx: &Drop_hierarchyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_library}.
 * @param ctx the parse tree
 */
fn enter_alter_library(&mut self, _ctx: &Alter_libraryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_library}.
 * @param ctx the parse tree
 */
fn exit_alter_library(&mut self, _ctx: &Alter_libraryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_java}.
 * @param ctx the parse tree
 */
fn enter_drop_java(&mut self, _ctx: &Drop_javaContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_java}.
 * @param ctx the parse tree
 */
fn exit_drop_java(&mut self, _ctx: &Drop_javaContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_library}.
 * @param ctx the parse tree
 */
fn enter_drop_library(&mut self, _ctx: &Drop_libraryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_library}.
 * @param ctx the parse tree
 */
fn exit_drop_library(&mut self, _ctx: &Drop_libraryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_java}.
 * @param ctx the parse tree
 */
fn enter_create_java(&mut self, _ctx: &Create_javaContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_java}.
 * @param ctx the parse tree
 */
fn exit_create_java(&mut self, _ctx: &Create_javaContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_library}.
 * @param ctx the parse tree
 */
fn enter_create_library(&mut self, _ctx: &Create_libraryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_library}.
 * @param ctx the parse tree
 */
fn exit_create_library(&mut self, _ctx: &Create_libraryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#plsql_library_source}.
 * @param ctx the parse tree
 */
fn enter_plsql_library_source(&mut self, _ctx: &Plsql_library_sourceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#plsql_library_source}.
 * @param ctx the parse tree
 */
fn exit_plsql_library_source(&mut self, _ctx: &Plsql_library_sourceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#credential_name}.
 * @param ctx the parse tree
 */
fn enter_credential_name(&mut self, _ctx: &Credential_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#credential_name}.
 * @param ctx the parse tree
 */
fn exit_credential_name(&mut self, _ctx: &Credential_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#library_editionable}.
 * @param ctx the parse tree
 */
fn enter_library_editionable(&mut self, _ctx: &Library_editionableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#library_editionable}.
 * @param ctx the parse tree
 */
fn exit_library_editionable(&mut self, _ctx: &Library_editionableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#library_debug}.
 * @param ctx the parse tree
 */
fn enter_library_debug(&mut self, _ctx: &Library_debugContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#library_debug}.
 * @param ctx the parse tree
 */
fn exit_library_debug(&mut self, _ctx: &Library_debugContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#compiler_parameters_clause}.
 * @param ctx the parse tree
 */
fn enter_compiler_parameters_clause(&mut self, _ctx: &Compiler_parameters_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#compiler_parameters_clause}.
 * @param ctx the parse tree
 */
fn exit_compiler_parameters_clause(&mut self, _ctx: &Compiler_parameters_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#parameter_value}.
 * @param ctx the parse tree
 */
fn enter_parameter_value(&mut self, _ctx: &Parameter_valueContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#parameter_value}.
 * @param ctx the parse tree
 */
fn exit_parameter_value(&mut self, _ctx: &Parameter_valueContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#library_name}.
 * @param ctx the parse tree
 */
fn enter_library_name(&mut self, _ctx: &Library_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#library_name}.
 * @param ctx the parse tree
 */
fn exit_library_name(&mut self, _ctx: &Library_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_dimension}.
 * @param ctx the parse tree
 */
fn enter_alter_dimension(&mut self, _ctx: &Alter_dimensionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_dimension}.
 * @param ctx the parse tree
 */
fn exit_alter_dimension(&mut self, _ctx: &Alter_dimensionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#level_clause}.
 * @param ctx the parse tree
 */
fn enter_level_clause(&mut self, _ctx: &Level_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#level_clause}.
 * @param ctx the parse tree
 */
fn exit_level_clause(&mut self, _ctx: &Level_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hierarchy_clause}.
 * @param ctx the parse tree
 */
fn enter_hierarchy_clause(&mut self, _ctx: &Hierarchy_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hierarchy_clause}.
 * @param ctx the parse tree
 */
fn exit_hierarchy_clause(&mut self, _ctx: &Hierarchy_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dimension_join_clause}.
 * @param ctx the parse tree
 */
fn enter_dimension_join_clause(&mut self, _ctx: &Dimension_join_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dimension_join_clause}.
 * @param ctx the parse tree
 */
fn exit_dimension_join_clause(&mut self, _ctx: &Dimension_join_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#attribute_clause}.
 * @param ctx the parse tree
 */
fn enter_attribute_clause(&mut self, _ctx: &Attribute_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#attribute_clause}.
 * @param ctx the parse tree
 */
fn exit_attribute_clause(&mut self, _ctx: &Attribute_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#extended_attribute_clause}.
 * @param ctx the parse tree
 */
fn enter_extended_attribute_clause(&mut self, _ctx: &Extended_attribute_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#extended_attribute_clause}.
 * @param ctx the parse tree
 */
fn exit_extended_attribute_clause(&mut self, _ctx: &Extended_attribute_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_one_or_more_sub_clause}.
 * @param ctx the parse tree
 */
fn enter_column_one_or_more_sub_clause(&mut self, _ctx: &Column_one_or_more_sub_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_one_or_more_sub_clause}.
 * @param ctx the parse tree
 */
fn exit_column_one_or_more_sub_clause(&mut self, _ctx: &Column_one_or_more_sub_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_view}.
 * @param ctx the parse tree
 */
fn enter_alter_view(&mut self, _ctx: &Alter_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_view}.
 * @param ctx the parse tree
 */
fn exit_alter_view(&mut self, _ctx: &Alter_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_view_editionable}.
 * @param ctx the parse tree
 */
fn enter_alter_view_editionable(&mut self, _ctx: &Alter_view_editionableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_view_editionable}.
 * @param ctx the parse tree
 */
fn exit_alter_view_editionable(&mut self, _ctx: &Alter_view_editionableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_view}.
 * @param ctx the parse tree
 */
fn enter_create_view(&mut self, _ctx: &Create_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_view}.
 * @param ctx the parse tree
 */
fn exit_create_view(&mut self, _ctx: &Create_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#editioning_clause}.
 * @param ctx the parse tree
 */
fn enter_editioning_clause(&mut self, _ctx: &Editioning_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#editioning_clause}.
 * @param ctx the parse tree
 */
fn exit_editioning_clause(&mut self, _ctx: &Editioning_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#view_options}.
 * @param ctx the parse tree
 */
fn enter_view_options(&mut self, _ctx: &View_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#view_options}.
 * @param ctx the parse tree
 */
fn exit_view_options(&mut self, _ctx: &View_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#view_alias_constraint}.
 * @param ctx the parse tree
 */
fn enter_view_alias_constraint(&mut self, _ctx: &View_alias_constraintContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#view_alias_constraint}.
 * @param ctx the parse tree
 */
fn exit_view_alias_constraint(&mut self, _ctx: &View_alias_constraintContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_view_clause}.
 * @param ctx the parse tree
 */
fn enter_object_view_clause(&mut self, _ctx: &Object_view_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_view_clause}.
 * @param ctx the parse tree
 */
fn exit_object_view_clause(&mut self, _ctx: &Object_view_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inline_constraint}.
 * @param ctx the parse tree
 */
fn enter_inline_constraint(&mut self, _ctx: &Inline_constraintContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inline_constraint}.
 * @param ctx the parse tree
 */
fn exit_inline_constraint(&mut self, _ctx: &Inline_constraintContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inline_ref_constraint}.
 * @param ctx the parse tree
 */
fn enter_inline_ref_constraint(&mut self, _ctx: &Inline_ref_constraintContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inline_ref_constraint}.
 * @param ctx the parse tree
 */
fn exit_inline_ref_constraint(&mut self, _ctx: &Inline_ref_constraintContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#out_of_line_ref_constraint}.
 * @param ctx the parse tree
 */
fn enter_out_of_line_ref_constraint(&mut self, _ctx: &Out_of_line_ref_constraintContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#out_of_line_ref_constraint}.
 * @param ctx the parse tree
 */
fn exit_out_of_line_ref_constraint(&mut self, _ctx: &Out_of_line_ref_constraintContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#out_of_line_constraint}.
 * @param ctx the parse tree
 */
fn enter_out_of_line_constraint(&mut self, _ctx: &Out_of_line_constraintContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#out_of_line_constraint}.
 * @param ctx the parse tree
 */
fn exit_out_of_line_constraint(&mut self, _ctx: &Out_of_line_constraintContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#constraint_state}.
 * @param ctx the parse tree
 */
fn enter_constraint_state(&mut self, _ctx: &Constraint_stateContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#constraint_state}.
 * @param ctx the parse tree
 */
fn exit_constraint_state(&mut self, _ctx: &Constraint_stateContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmltype_view_clause}.
 * @param ctx the parse tree
 */
fn enter_xmltype_view_clause(&mut self, _ctx: &Xmltype_view_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmltype_view_clause}.
 * @param ctx the parse tree
 */
fn exit_xmltype_view_clause(&mut self, _ctx: &Xmltype_view_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_schema_spec}.
 * @param ctx the parse tree
 */
fn enter_xml_schema_spec(&mut self, _ctx: &Xml_schema_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_schema_spec}.
 * @param ctx the parse tree
 */
fn exit_xml_schema_spec(&mut self, _ctx: &Xml_schema_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_schema_url}.
 * @param ctx the parse tree
 */
fn enter_xml_schema_url(&mut self, _ctx: &Xml_schema_urlContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_schema_url}.
 * @param ctx the parse tree
 */
fn exit_xml_schema_url(&mut self, _ctx: &Xml_schema_urlContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#element}.
 * @param ctx the parse tree
 */
fn enter_element(&mut self, _ctx: &ElementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#element}.
 * @param ctx the parse tree
 */
fn exit_element(&mut self, _ctx: &ElementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_tablespace}.
 * @param ctx the parse tree
 */
fn enter_alter_tablespace(&mut self, _ctx: &Alter_tablespaceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_tablespace}.
 * @param ctx the parse tree
 */
fn exit_alter_tablespace(&mut self, _ctx: &Alter_tablespaceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#datafile_tempfile_clauses}.
 * @param ctx the parse tree
 */
fn enter_datafile_tempfile_clauses(&mut self, _ctx: &Datafile_tempfile_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#datafile_tempfile_clauses}.
 * @param ctx the parse tree
 */
fn exit_datafile_tempfile_clauses(&mut self, _ctx: &Datafile_tempfile_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_logging_clauses}.
 * @param ctx the parse tree
 */
fn enter_tablespace_logging_clauses(&mut self, _ctx: &Tablespace_logging_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_logging_clauses}.
 * @param ctx the parse tree
 */
fn exit_tablespace_logging_clauses(&mut self, _ctx: &Tablespace_logging_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_group_clause}.
 * @param ctx the parse tree
 */
fn enter_tablespace_group_clause(&mut self, _ctx: &Tablespace_group_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_group_clause}.
 * @param ctx the parse tree
 */
fn exit_tablespace_group_clause(&mut self, _ctx: &Tablespace_group_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_group_name}.
 * @param ctx the parse tree
 */
fn enter_tablespace_group_name(&mut self, _ctx: &Tablespace_group_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_group_name}.
 * @param ctx the parse tree
 */
fn exit_tablespace_group_name(&mut self, _ctx: &Tablespace_group_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_state_clauses}.
 * @param ctx the parse tree
 */
fn enter_tablespace_state_clauses(&mut self, _ctx: &Tablespace_state_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_state_clauses}.
 * @param ctx the parse tree
 */
fn exit_tablespace_state_clauses(&mut self, _ctx: &Tablespace_state_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#flashback_mode_clause}.
 * @param ctx the parse tree
 */
fn enter_flashback_mode_clause(&mut self, _ctx: &Flashback_mode_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#flashback_mode_clause}.
 * @param ctx the parse tree
 */
fn exit_flashback_mode_clause(&mut self, _ctx: &Flashback_mode_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#new_tablespace_name}.
 * @param ctx the parse tree
 */
fn enter_new_tablespace_name(&mut self, _ctx: &New_tablespace_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#new_tablespace_name}.
 * @param ctx the parse tree
 */
fn exit_new_tablespace_name(&mut self, _ctx: &New_tablespace_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_tablespace}.
 * @param ctx the parse tree
 */
fn enter_create_tablespace(&mut self, _ctx: &Create_tablespaceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_tablespace}.
 * @param ctx the parse tree
 */
fn exit_create_tablespace(&mut self, _ctx: &Create_tablespaceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#permanent_tablespace_clause}.
 * @param ctx the parse tree
 */
fn enter_permanent_tablespace_clause(&mut self, _ctx: &Permanent_tablespace_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#permanent_tablespace_clause}.
 * @param ctx the parse tree
 */
fn exit_permanent_tablespace_clause(&mut self, _ctx: &Permanent_tablespace_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_encryption_spec}.
 * @param ctx the parse tree
 */
fn enter_tablespace_encryption_spec(&mut self, _ctx: &Tablespace_encryption_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_encryption_spec}.
 * @param ctx the parse tree
 */
fn exit_tablespace_encryption_spec(&mut self, _ctx: &Tablespace_encryption_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#logging_clause}.
 * @param ctx the parse tree
 */
fn enter_logging_clause(&mut self, _ctx: &Logging_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#logging_clause}.
 * @param ctx the parse tree
 */
fn exit_logging_clause(&mut self, _ctx: &Logging_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#extent_management_clause}.
 * @param ctx the parse tree
 */
fn enter_extent_management_clause(&mut self, _ctx: &Extent_management_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#extent_management_clause}.
 * @param ctx the parse tree
 */
fn exit_extent_management_clause(&mut self, _ctx: &Extent_management_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#segment_management_clause}.
 * @param ctx the parse tree
 */
fn enter_segment_management_clause(&mut self, _ctx: &Segment_management_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#segment_management_clause}.
 * @param ctx the parse tree
 */
fn exit_segment_management_clause(&mut self, _ctx: &Segment_management_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#temporary_tablespace_clause}.
 * @param ctx the parse tree
 */
fn enter_temporary_tablespace_clause(&mut self, _ctx: &Temporary_tablespace_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#temporary_tablespace_clause}.
 * @param ctx the parse tree
 */
fn exit_temporary_tablespace_clause(&mut self, _ctx: &Temporary_tablespace_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#undo_tablespace_clause}.
 * @param ctx the parse tree
 */
fn enter_undo_tablespace_clause(&mut self, _ctx: &Undo_tablespace_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#undo_tablespace_clause}.
 * @param ctx the parse tree
 */
fn exit_undo_tablespace_clause(&mut self, _ctx: &Undo_tablespace_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_retention_clause}.
 * @param ctx the parse tree
 */
fn enter_tablespace_retention_clause(&mut self, _ctx: &Tablespace_retention_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_retention_clause}.
 * @param ctx the parse tree
 */
fn exit_tablespace_retention_clause(&mut self, _ctx: &Tablespace_retention_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_tablespace_set}.
 * @param ctx the parse tree
 */
fn enter_create_tablespace_set(&mut self, _ctx: &Create_tablespace_setContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_tablespace_set}.
 * @param ctx the parse tree
 */
fn exit_create_tablespace_set(&mut self, _ctx: &Create_tablespace_setContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#permanent_tablespace_attrs}.
 * @param ctx the parse tree
 */
fn enter_permanent_tablespace_attrs(&mut self, _ctx: &Permanent_tablespace_attrsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#permanent_tablespace_attrs}.
 * @param ctx the parse tree
 */
fn exit_permanent_tablespace_attrs(&mut self, _ctx: &Permanent_tablespace_attrsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_encryption_clause}.
 * @param ctx the parse tree
 */
fn enter_tablespace_encryption_clause(&mut self, _ctx: &Tablespace_encryption_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_encryption_clause}.
 * @param ctx the parse tree
 */
fn exit_tablespace_encryption_clause(&mut self, _ctx: &Tablespace_encryption_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_tablespace_params}.
 * @param ctx the parse tree
 */
fn enter_default_tablespace_params(&mut self, _ctx: &Default_tablespace_paramsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_tablespace_params}.
 * @param ctx the parse tree
 */
fn exit_default_tablespace_params(&mut self, _ctx: &Default_tablespace_paramsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_table_compression}.
 * @param ctx the parse tree
 */
fn enter_default_table_compression(&mut self, _ctx: &Default_table_compressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_table_compression}.
 * @param ctx the parse tree
 */
fn exit_default_table_compression(&mut self, _ctx: &Default_table_compressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#low_high}.
 * @param ctx the parse tree
 */
fn enter_low_high(&mut self, _ctx: &Low_highContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#low_high}.
 * @param ctx the parse tree
 */
fn exit_low_high(&mut self, _ctx: &Low_highContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_index_compression}.
 * @param ctx the parse tree
 */
fn enter_default_index_compression(&mut self, _ctx: &Default_index_compressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_index_compression}.
 * @param ctx the parse tree
 */
fn exit_default_index_compression(&mut self, _ctx: &Default_index_compressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inmmemory_clause}.
 * @param ctx the parse tree
 */
fn enter_inmmemory_clause(&mut self, _ctx: &Inmmemory_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inmmemory_clause}.
 * @param ctx the parse tree
 */
fn exit_inmmemory_clause(&mut self, _ctx: &Inmmemory_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#datafile_specification}.
 * @param ctx the parse tree
 */
fn enter_datafile_specification(&mut self, _ctx: &Datafile_specificationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#datafile_specification}.
 * @param ctx the parse tree
 */
fn exit_datafile_specification(&mut self, _ctx: &Datafile_specificationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tempfile_specification}.
 * @param ctx the parse tree
 */
fn enter_tempfile_specification(&mut self, _ctx: &Tempfile_specificationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tempfile_specification}.
 * @param ctx the parse tree
 */
fn exit_tempfile_specification(&mut self, _ctx: &Tempfile_specificationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#datafile_tempfile_spec}.
 * @param ctx the parse tree
 */
fn enter_datafile_tempfile_spec(&mut self, _ctx: &Datafile_tempfile_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#datafile_tempfile_spec}.
 * @param ctx the parse tree
 */
fn exit_datafile_tempfile_spec(&mut self, _ctx: &Datafile_tempfile_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#redo_log_file_spec}.
 * @param ctx the parse tree
 */
fn enter_redo_log_file_spec(&mut self, _ctx: &Redo_log_file_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#redo_log_file_spec}.
 * @param ctx the parse tree
 */
fn exit_redo_log_file_spec(&mut self, _ctx: &Redo_log_file_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#autoextend_clause}.
 * @param ctx the parse tree
 */
fn enter_autoextend_clause(&mut self, _ctx: &Autoextend_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#autoextend_clause}.
 * @param ctx the parse tree
 */
fn exit_autoextend_clause(&mut self, _ctx: &Autoextend_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#maxsize_clause}.
 * @param ctx the parse tree
 */
fn enter_maxsize_clause(&mut self, _ctx: &Maxsize_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#maxsize_clause}.
 * @param ctx the parse tree
 */
fn exit_maxsize_clause(&mut self, _ctx: &Maxsize_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#build_clause}.
 * @param ctx the parse tree
 */
fn enter_build_clause(&mut self, _ctx: &Build_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#build_clause}.
 * @param ctx the parse tree
 */
fn exit_build_clause(&mut self, _ctx: &Build_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#parallel_clause}.
 * @param ctx the parse tree
 */
fn enter_parallel_clause(&mut self, _ctx: &Parallel_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#parallel_clause}.
 * @param ctx the parse tree
 */
fn exit_parallel_clause(&mut self, _ctx: &Parallel_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#parallel_instances_clause}.
 * @param ctx the parse tree
 */
fn enter_parallel_instances_clause(&mut self, _ctx: &Parallel_instances_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#parallel_instances_clause}.
 * @param ctx the parse tree
 */
fn exit_parallel_instances_clause(&mut self, _ctx: &Parallel_instances_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_materialized_view}.
 * @param ctx the parse tree
 */
fn enter_alter_materialized_view(&mut self, _ctx: &Alter_materialized_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_materialized_view}.
 * @param ctx the parse tree
 */
fn exit_alter_materialized_view(&mut self, _ctx: &Alter_materialized_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_mv_option1}.
 * @param ctx the parse tree
 */
fn enter_alter_mv_option1(&mut self, _ctx: &Alter_mv_option1Context<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_mv_option1}.
 * @param ctx the parse tree
 */
fn exit_alter_mv_option1(&mut self, _ctx: &Alter_mv_option1Context<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_mv_refresh}.
 * @param ctx the parse tree
 */
fn enter_alter_mv_refresh(&mut self, _ctx: &Alter_mv_refreshContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_mv_refresh}.
 * @param ctx the parse tree
 */
fn exit_alter_mv_refresh(&mut self, _ctx: &Alter_mv_refreshContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rollback_segment}.
 * @param ctx the parse tree
 */
fn enter_rollback_segment(&mut self, _ctx: &Rollback_segmentContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rollback_segment}.
 * @param ctx the parse tree
 */
fn exit_rollback_segment(&mut self, _ctx: &Rollback_segmentContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_mv_column_clause}.
 * @param ctx the parse tree
 */
fn enter_modify_mv_column_clause(&mut self, _ctx: &Modify_mv_column_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_mv_column_clause}.
 * @param ctx the parse tree
 */
fn exit_modify_mv_column_clause(&mut self, _ctx: &Modify_mv_column_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_materialized_view_log}.
 * @param ctx the parse tree
 */
fn enter_alter_materialized_view_log(&mut self, _ctx: &Alter_materialized_view_logContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_materialized_view_log}.
 * @param ctx the parse tree
 */
fn exit_alter_materialized_view_log(&mut self, _ctx: &Alter_materialized_view_logContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_mv_log_column_clause}.
 * @param ctx the parse tree
 */
fn enter_add_mv_log_column_clause(&mut self, _ctx: &Add_mv_log_column_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_mv_log_column_clause}.
 * @param ctx the parse tree
 */
fn exit_add_mv_log_column_clause(&mut self, _ctx: &Add_mv_log_column_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#move_mv_log_clause}.
 * @param ctx the parse tree
 */
fn enter_move_mv_log_clause(&mut self, _ctx: &Move_mv_log_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#move_mv_log_clause}.
 * @param ctx the parse tree
 */
fn exit_move_mv_log_clause(&mut self, _ctx: &Move_mv_log_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#mv_log_augmentation}.
 * @param ctx the parse tree
 */
fn enter_mv_log_augmentation(&mut self, _ctx: &Mv_log_augmentationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#mv_log_augmentation}.
 * @param ctx the parse tree
 */
fn exit_mv_log_augmentation(&mut self, _ctx: &Mv_log_augmentationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_materialized_view_log}.
 * @param ctx the parse tree
 */
fn enter_create_materialized_view_log(&mut self, _ctx: &Create_materialized_view_logContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_materialized_view_log}.
 * @param ctx the parse tree
 */
fn exit_create_materialized_view_log(&mut self, _ctx: &Create_materialized_view_logContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#new_values_clause}.
 * @param ctx the parse tree
 */
fn enter_new_values_clause(&mut self, _ctx: &New_values_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#new_values_clause}.
 * @param ctx the parse tree
 */
fn exit_new_values_clause(&mut self, _ctx: &New_values_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#mv_log_purge_clause}.
 * @param ctx the parse tree
 */
fn enter_mv_log_purge_clause(&mut self, _ctx: &Mv_log_purge_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#mv_log_purge_clause}.
 * @param ctx the parse tree
 */
fn exit_mv_log_purge_clause(&mut self, _ctx: &Mv_log_purge_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_materialized_zonemap}.
 * @param ctx the parse tree
 */
fn enter_create_materialized_zonemap(&mut self, _ctx: &Create_materialized_zonemapContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_materialized_zonemap}.
 * @param ctx the parse tree
 */
fn exit_create_materialized_zonemap(&mut self, _ctx: &Create_materialized_zonemapContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_materialized_zonemap}.
 * @param ctx the parse tree
 */
fn enter_alter_materialized_zonemap(&mut self, _ctx: &Alter_materialized_zonemapContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_materialized_zonemap}.
 * @param ctx the parse tree
 */
fn exit_alter_materialized_zonemap(&mut self, _ctx: &Alter_materialized_zonemapContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_materialized_zonemap}.
 * @param ctx the parse tree
 */
fn enter_drop_materialized_zonemap(&mut self, _ctx: &Drop_materialized_zonemapContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_materialized_zonemap}.
 * @param ctx the parse tree
 */
fn exit_drop_materialized_zonemap(&mut self, _ctx: &Drop_materialized_zonemapContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#zonemap_refresh_clause}.
 * @param ctx the parse tree
 */
fn enter_zonemap_refresh_clause(&mut self, _ctx: &Zonemap_refresh_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#zonemap_refresh_clause}.
 * @param ctx the parse tree
 */
fn exit_zonemap_refresh_clause(&mut self, _ctx: &Zonemap_refresh_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#zonemap_attributes}.
 * @param ctx the parse tree
 */
fn enter_zonemap_attributes(&mut self, _ctx: &Zonemap_attributesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#zonemap_attributes}.
 * @param ctx the parse tree
 */
fn exit_zonemap_attributes(&mut self, _ctx: &Zonemap_attributesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#zonemap_name}.
 * @param ctx the parse tree
 */
fn enter_zonemap_name(&mut self, _ctx: &Zonemap_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#zonemap_name}.
 * @param ctx the parse tree
 */
fn exit_zonemap_name(&mut self, _ctx: &Zonemap_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#operator_name}.
 * @param ctx the parse tree
 */
fn enter_operator_name(&mut self, _ctx: &Operator_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#operator_name}.
 * @param ctx the parse tree
 */
fn exit_operator_name(&mut self, _ctx: &Operator_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#operator_function_name}.
 * @param ctx the parse tree
 */
fn enter_operator_function_name(&mut self, _ctx: &Operator_function_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#operator_function_name}.
 * @param ctx the parse tree
 */
fn exit_operator_function_name(&mut self, _ctx: &Operator_function_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_zonemap_on_table}.
 * @param ctx the parse tree
 */
fn enter_create_zonemap_on_table(&mut self, _ctx: &Create_zonemap_on_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_zonemap_on_table}.
 * @param ctx the parse tree
 */
fn exit_create_zonemap_on_table(&mut self, _ctx: &Create_zonemap_on_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_zonemap_as_subquery}.
 * @param ctx the parse tree
 */
fn enter_create_zonemap_as_subquery(&mut self, _ctx: &Create_zonemap_as_subqueryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_zonemap_as_subquery}.
 * @param ctx the parse tree
 */
fn exit_create_zonemap_as_subquery(&mut self, _ctx: &Create_zonemap_as_subqueryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_operator}.
 * @param ctx the parse tree
 */
fn enter_alter_operator(&mut self, _ctx: &Alter_operatorContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_operator}.
 * @param ctx the parse tree
 */
fn exit_alter_operator(&mut self, _ctx: &Alter_operatorContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_operator}.
 * @param ctx the parse tree
 */
fn enter_drop_operator(&mut self, _ctx: &Drop_operatorContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_operator}.
 * @param ctx the parse tree
 */
fn exit_drop_operator(&mut self, _ctx: &Drop_operatorContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_operator}.
 * @param ctx the parse tree
 */
fn enter_create_operator(&mut self, _ctx: &Create_operatorContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_operator}.
 * @param ctx the parse tree
 */
fn exit_create_operator(&mut self, _ctx: &Create_operatorContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#binding_clause}.
 * @param ctx the parse tree
 */
fn enter_binding_clause(&mut self, _ctx: &Binding_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#binding_clause}.
 * @param ctx the parse tree
 */
fn exit_binding_clause(&mut self, _ctx: &Binding_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_binding_clause}.
 * @param ctx the parse tree
 */
fn enter_add_binding_clause(&mut self, _ctx: &Add_binding_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_binding_clause}.
 * @param ctx the parse tree
 */
fn exit_add_binding_clause(&mut self, _ctx: &Add_binding_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#implementation_clause}.
 * @param ctx the parse tree
 */
fn enter_implementation_clause(&mut self, _ctx: &Implementation_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#implementation_clause}.
 * @param ctx the parse tree
 */
fn exit_implementation_clause(&mut self, _ctx: &Implementation_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#primary_operator_list}.
 * @param ctx the parse tree
 */
fn enter_primary_operator_list(&mut self, _ctx: &Primary_operator_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#primary_operator_list}.
 * @param ctx the parse tree
 */
fn exit_primary_operator_list(&mut self, _ctx: &Primary_operator_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#primary_operator_item}.
 * @param ctx the parse tree
 */
fn enter_primary_operator_item(&mut self, _ctx: &Primary_operator_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#primary_operator_item}.
 * @param ctx the parse tree
 */
fn exit_primary_operator_item(&mut self, _ctx: &Primary_operator_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#operator_context_clause}.
 * @param ctx the parse tree
 */
fn enter_operator_context_clause(&mut self, _ctx: &Operator_context_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#operator_context_clause}.
 * @param ctx the parse tree
 */
fn exit_operator_context_clause(&mut self, _ctx: &Operator_context_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#using_function_clause}.
 * @param ctx the parse tree
 */
fn enter_using_function_clause(&mut self, _ctx: &Using_function_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#using_function_clause}.
 * @param ctx the parse tree
 */
fn exit_using_function_clause(&mut self, _ctx: &Using_function_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_binding_clause}.
 * @param ctx the parse tree
 */
fn enter_drop_binding_clause(&mut self, _ctx: &Drop_binding_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_binding_clause}.
 * @param ctx the parse tree
 */
fn exit_drop_binding_clause(&mut self, _ctx: &Drop_binding_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_materialized_view}.
 * @param ctx the parse tree
 */
fn enter_create_materialized_view(&mut self, _ctx: &Create_materialized_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_materialized_view}.
 * @param ctx the parse tree
 */
fn exit_create_materialized_view(&mut self, _ctx: &Create_materialized_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#scoped_table_ref_constraint}.
 * @param ctx the parse tree
 */
fn enter_scoped_table_ref_constraint(&mut self, _ctx: &Scoped_table_ref_constraintContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#scoped_table_ref_constraint}.
 * @param ctx the parse tree
 */
fn exit_scoped_table_ref_constraint(&mut self, _ctx: &Scoped_table_ref_constraintContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#mv_column_alias}.
 * @param ctx the parse tree
 */
fn enter_mv_column_alias(&mut self, _ctx: &Mv_column_aliasContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#mv_column_alias}.
 * @param ctx the parse tree
 */
fn exit_mv_column_alias(&mut self, _ctx: &Mv_column_aliasContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_mv_refresh}.
 * @param ctx the parse tree
 */
fn enter_create_mv_refresh(&mut self, _ctx: &Create_mv_refreshContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_mv_refresh}.
 * @param ctx the parse tree
 */
fn exit_create_mv_refresh(&mut self, _ctx: &Create_mv_refreshContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_materialized_view}.
 * @param ctx the parse tree
 */
fn enter_drop_materialized_view(&mut self, _ctx: &Drop_materialized_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_materialized_view}.
 * @param ctx the parse tree
 */
fn exit_drop_materialized_view(&mut self, _ctx: &Drop_materialized_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_materialized_view_log}.
 * @param ctx the parse tree
 */
fn enter_drop_materialized_view_log(&mut self, _ctx: &Drop_materialized_view_logContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_materialized_view_log}.
 * @param ctx the parse tree
 */
fn exit_drop_materialized_view_log(&mut self, _ctx: &Drop_materialized_view_logContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_context}.
 * @param ctx the parse tree
 */
fn enter_create_context(&mut self, _ctx: &Create_contextContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_context}.
 * @param ctx the parse tree
 */
fn exit_create_context(&mut self, _ctx: &Create_contextContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#oracle_namespace}.
 * @param ctx the parse tree
 */
fn enter_oracle_namespace(&mut self, _ctx: &Oracle_namespaceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#oracle_namespace}.
 * @param ctx the parse tree
 */
fn exit_oracle_namespace(&mut self, _ctx: &Oracle_namespaceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_cluster}.
 * @param ctx the parse tree
 */
fn enter_create_cluster(&mut self, _ctx: &Create_clusterContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_cluster}.
 * @param ctx the parse tree
 */
fn exit_create_cluster(&mut self, _ctx: &Create_clusterContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_profile}.
 * @param ctx the parse tree
 */
fn enter_create_profile(&mut self, _ctx: &Create_profileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_profile}.
 * @param ctx the parse tree
 */
fn exit_create_profile(&mut self, _ctx: &Create_profileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#resource_parameters}.
 * @param ctx the parse tree
 */
fn enter_resource_parameters(&mut self, _ctx: &Resource_parametersContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#resource_parameters}.
 * @param ctx the parse tree
 */
fn exit_resource_parameters(&mut self, _ctx: &Resource_parametersContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#password_parameters}.
 * @param ctx the parse tree
 */
fn enter_password_parameters(&mut self, _ctx: &Password_parametersContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#password_parameters}.
 * @param ctx the parse tree
 */
fn exit_password_parameters(&mut self, _ctx: &Password_parametersContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_lockdown_profile}.
 * @param ctx the parse tree
 */
fn enter_create_lockdown_profile(&mut self, _ctx: &Create_lockdown_profileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_lockdown_profile}.
 * @param ctx the parse tree
 */
fn exit_create_lockdown_profile(&mut self, _ctx: &Create_lockdown_profileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#static_base_profile}.
 * @param ctx the parse tree
 */
fn enter_static_base_profile(&mut self, _ctx: &Static_base_profileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#static_base_profile}.
 * @param ctx the parse tree
 */
fn exit_static_base_profile(&mut self, _ctx: &Static_base_profileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dynamic_base_profile}.
 * @param ctx the parse tree
 */
fn enter_dynamic_base_profile(&mut self, _ctx: &Dynamic_base_profileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dynamic_base_profile}.
 * @param ctx the parse tree
 */
fn exit_dynamic_base_profile(&mut self, _ctx: &Dynamic_base_profileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_outline}.
 * @param ctx the parse tree
 */
fn enter_create_outline(&mut self, _ctx: &Create_outlineContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_outline}.
 * @param ctx the parse tree
 */
fn exit_create_outline(&mut self, _ctx: &Create_outlineContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_restore_point}.
 * @param ctx the parse tree
 */
fn enter_create_restore_point(&mut self, _ctx: &Create_restore_pointContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_restore_point}.
 * @param ctx the parse tree
 */
fn exit_create_restore_point(&mut self, _ctx: &Create_restore_pointContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_role}.
 * @param ctx the parse tree
 */
fn enter_create_role(&mut self, _ctx: &Create_roleContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_role}.
 * @param ctx the parse tree
 */
fn exit_create_role(&mut self, _ctx: &Create_roleContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_table}.
 * @param ctx the parse tree
 */
fn enter_create_table(&mut self, _ctx: &Create_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_table}.
 * @param ctx the parse tree
 */
fn exit_create_table(&mut self, _ctx: &Create_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmltype_table}.
 * @param ctx the parse tree
 */
fn enter_xmltype_table(&mut self, _ctx: &Xmltype_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmltype_table}.
 * @param ctx the parse tree
 */
fn exit_xmltype_table(&mut self, _ctx: &Xmltype_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmltype_virtual_columns}.
 * @param ctx the parse tree
 */
fn enter_xmltype_virtual_columns(&mut self, _ctx: &Xmltype_virtual_columnsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmltype_virtual_columns}.
 * @param ctx the parse tree
 */
fn exit_xmltype_virtual_columns(&mut self, _ctx: &Xmltype_virtual_columnsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmltype_column_properties}.
 * @param ctx the parse tree
 */
fn enter_xmltype_column_properties(&mut self, _ctx: &Xmltype_column_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmltype_column_properties}.
 * @param ctx the parse tree
 */
fn exit_xmltype_column_properties(&mut self, _ctx: &Xmltype_column_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmltype_storage}.
 * @param ctx the parse tree
 */
fn enter_xmltype_storage(&mut self, _ctx: &Xmltype_storageContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmltype_storage}.
 * @param ctx the parse tree
 */
fn exit_xmltype_storage(&mut self, _ctx: &Xmltype_storageContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmlschema_spec}.
 * @param ctx the parse tree
 */
fn enter_xmlschema_spec(&mut self, _ctx: &Xmlschema_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmlschema_spec}.
 * @param ctx the parse tree
 */
fn exit_xmlschema_spec(&mut self, _ctx: &Xmlschema_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_table}.
 * @param ctx the parse tree
 */
fn enter_object_table(&mut self, _ctx: &Object_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_table}.
 * @param ctx the parse tree
 */
fn exit_object_table(&mut self, _ctx: &Object_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_type}.
 * @param ctx the parse tree
 */
fn enter_object_type(&mut self, _ctx: &Object_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_type}.
 * @param ctx the parse tree
 */
fn exit_object_type(&mut self, _ctx: &Object_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#oid_index_clause}.
 * @param ctx the parse tree
 */
fn enter_oid_index_clause(&mut self, _ctx: &Oid_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#oid_index_clause}.
 * @param ctx the parse tree
 */
fn exit_oid_index_clause(&mut self, _ctx: &Oid_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#oid_clause}.
 * @param ctx the parse tree
 */
fn enter_oid_clause(&mut self, _ctx: &Oid_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#oid_clause}.
 * @param ctx the parse tree
 */
fn exit_oid_clause(&mut self, _ctx: &Oid_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_properties}.
 * @param ctx the parse tree
 */
fn enter_object_properties(&mut self, _ctx: &Object_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_properties}.
 * @param ctx the parse tree
 */
fn exit_object_properties(&mut self, _ctx: &Object_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_table_substitution}.
 * @param ctx the parse tree
 */
fn enter_object_table_substitution(&mut self, _ctx: &Object_table_substitutionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_table_substitution}.
 * @param ctx the parse tree
 */
fn exit_object_table_substitution(&mut self, _ctx: &Object_table_substitutionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#relational_table}.
 * @param ctx the parse tree
 */
fn enter_relational_table(&mut self, _ctx: &Relational_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#relational_table}.
 * @param ctx the parse tree
 */
fn exit_relational_table(&mut self, _ctx: &Relational_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#relational_table_properties}.
 * @param ctx the parse tree
 */
fn enter_relational_table_properties(&mut self, _ctx: &Relational_table_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#relational_table_properties}.
 * @param ctx the parse tree
 */
fn exit_relational_table_properties(&mut self, _ctx: &Relational_table_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#relational_table_property}.
 * @param ctx the parse tree
 */
fn enter_relational_table_property(&mut self, _ctx: &Relational_table_propertyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#relational_table_property}.
 * @param ctx the parse tree
 */
fn exit_relational_table_property(&mut self, _ctx: &Relational_table_propertyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#immutable_table_clauses}.
 * @param ctx the parse tree
 */
fn enter_immutable_table_clauses(&mut self, _ctx: &Immutable_table_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#immutable_table_clauses}.
 * @param ctx the parse tree
 */
fn exit_immutable_table_clauses(&mut self, _ctx: &Immutable_table_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#immutable_table_no_drop_clause}.
 * @param ctx the parse tree
 */
fn enter_immutable_table_no_drop_clause(&mut self, _ctx: &Immutable_table_no_drop_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#immutable_table_no_drop_clause}.
 * @param ctx the parse tree
 */
fn exit_immutable_table_no_drop_clause(&mut self, _ctx: &Immutable_table_no_drop_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#immutable_table_no_delete_clause}.
 * @param ctx the parse tree
 */
fn enter_immutable_table_no_delete_clause(&mut self, _ctx: &Immutable_table_no_delete_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#immutable_table_no_delete_clause}.
 * @param ctx the parse tree
 */
fn exit_immutable_table_no_delete_clause(&mut self, _ctx: &Immutable_table_no_delete_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#blockchain_table_clauses}.
 * @param ctx the parse tree
 */
fn enter_blockchain_table_clauses(&mut self, _ctx: &Blockchain_table_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#blockchain_table_clauses}.
 * @param ctx the parse tree
 */
fn exit_blockchain_table_clauses(&mut self, _ctx: &Blockchain_table_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#blockchain_drop_table_clause}.
 * @param ctx the parse tree
 */
fn enter_blockchain_drop_table_clause(&mut self, _ctx: &Blockchain_drop_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#blockchain_drop_table_clause}.
 * @param ctx the parse tree
 */
fn exit_blockchain_drop_table_clause(&mut self, _ctx: &Blockchain_drop_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#blockchain_row_retention_clause}.
 * @param ctx the parse tree
 */
fn enter_blockchain_row_retention_clause(&mut self, _ctx: &Blockchain_row_retention_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#blockchain_row_retention_clause}.
 * @param ctx the parse tree
 */
fn exit_blockchain_row_retention_clause(&mut self, _ctx: &Blockchain_row_retention_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#blockchain_hash_and_data_format_clause}.
 * @param ctx the parse tree
 */
fn enter_blockchain_hash_and_data_format_clause(&mut self, _ctx: &Blockchain_hash_and_data_format_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#blockchain_hash_and_data_format_clause}.
 * @param ctx the parse tree
 */
fn exit_blockchain_hash_and_data_format_clause(&mut self, _ctx: &Blockchain_hash_and_data_format_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#collation_name}.
 * @param ctx the parse tree
 */
fn enter_collation_name(&mut self, _ctx: &Collation_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#collation_name}.
 * @param ctx the parse tree
 */
fn exit_collation_name(&mut self, _ctx: &Collation_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_properties}.
 * @param ctx the parse tree
 */
fn enter_table_properties(&mut self, _ctx: &Table_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_properties}.
 * @param ctx the parse tree
 */
fn exit_table_properties(&mut self, _ctx: &Table_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#read_only_clause}.
 * @param ctx the parse tree
 */
fn enter_read_only_clause(&mut self, _ctx: &Read_only_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#read_only_clause}.
 * @param ctx the parse tree
 */
fn exit_read_only_clause(&mut self, _ctx: &Read_only_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#indexing_clause}.
 * @param ctx the parse tree
 */
fn enter_indexing_clause(&mut self, _ctx: &Indexing_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#indexing_clause}.
 * @param ctx the parse tree
 */
fn exit_indexing_clause(&mut self, _ctx: &Indexing_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#attribute_clustering_clause}.
 * @param ctx the parse tree
 */
fn enter_attribute_clustering_clause(&mut self, _ctx: &Attribute_clustering_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#attribute_clustering_clause}.
 * @param ctx the parse tree
 */
fn exit_attribute_clustering_clause(&mut self, _ctx: &Attribute_clustering_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#clustering_join}.
 * @param ctx the parse tree
 */
fn enter_clustering_join(&mut self, _ctx: &Clustering_joinContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#clustering_join}.
 * @param ctx the parse tree
 */
fn exit_clustering_join(&mut self, _ctx: &Clustering_joinContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#clustering_join_item}.
 * @param ctx the parse tree
 */
fn enter_clustering_join_item(&mut self, _ctx: &Clustering_join_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#clustering_join_item}.
 * @param ctx the parse tree
 */
fn exit_clustering_join_item(&mut self, _ctx: &Clustering_join_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#equijoin_condition}.
 * @param ctx the parse tree
 */
fn enter_equijoin_condition(&mut self, _ctx: &Equijoin_conditionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#equijoin_condition}.
 * @param ctx the parse tree
 */
fn exit_equijoin_condition(&mut self, _ctx: &Equijoin_conditionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cluster_clause}.
 * @param ctx the parse tree
 */
fn enter_cluster_clause(&mut self, _ctx: &Cluster_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cluster_clause}.
 * @param ctx the parse tree
 */
fn exit_cluster_clause(&mut self, _ctx: &Cluster_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#clustering_columns}.
 * @param ctx the parse tree
 */
fn enter_clustering_columns(&mut self, _ctx: &Clustering_columnsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#clustering_columns}.
 * @param ctx the parse tree
 */
fn exit_clustering_columns(&mut self, _ctx: &Clustering_columnsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#clustering_column_group}.
 * @param ctx the parse tree
 */
fn enter_clustering_column_group(&mut self, _ctx: &Clustering_column_groupContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#clustering_column_group}.
 * @param ctx the parse tree
 */
fn exit_clustering_column_group(&mut self, _ctx: &Clustering_column_groupContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#yes_no}.
 * @param ctx the parse tree
 */
fn enter_yes_no(&mut self, _ctx: &Yes_noContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#yes_no}.
 * @param ctx the parse tree
 */
fn exit_yes_no(&mut self, _ctx: &Yes_noContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#zonemap_clause}.
 * @param ctx the parse tree
 */
fn enter_zonemap_clause(&mut self, _ctx: &Zonemap_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#zonemap_clause}.
 * @param ctx the parse tree
 */
fn exit_zonemap_clause(&mut self, _ctx: &Zonemap_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#logical_replication_clause}.
 * @param ctx the parse tree
 */
fn enter_logical_replication_clause(&mut self, _ctx: &Logical_replication_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#logical_replication_clause}.
 * @param ctx the parse tree
 */
fn exit_logical_replication_clause(&mut self, _ctx: &Logical_replication_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_name}.
 * @param ctx the parse tree
 */
fn enter_table_name(&mut self, _ctx: &Table_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_name}.
 * @param ctx the parse tree
 */
fn exit_table_name(&mut self, _ctx: &Table_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#relational_property}.
 * @param ctx the parse tree
 */
fn enter_relational_property(&mut self, _ctx: &Relational_propertyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#relational_property}.
 * @param ctx the parse tree
 */
fn exit_relational_property(&mut self, _ctx: &Relational_propertyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_partitioning_clauses}.
 * @param ctx the parse tree
 */
fn enter_table_partitioning_clauses(&mut self, _ctx: &Table_partitioning_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_partitioning_clauses}.
 * @param ctx the parse tree
 */
fn exit_table_partitioning_clauses(&mut self, _ctx: &Table_partitioning_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#range_partitions}.
 * @param ctx the parse tree
 */
fn enter_range_partitions(&mut self, _ctx: &Range_partitionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#range_partitions}.
 * @param ctx the parse tree
 */
fn exit_range_partitions(&mut self, _ctx: &Range_partitionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#list_partitions}.
 * @param ctx the parse tree
 */
fn enter_list_partitions(&mut self, _ctx: &List_partitionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#list_partitions}.
 * @param ctx the parse tree
 */
fn exit_list_partitions(&mut self, _ctx: &List_partitionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hash_partitions}.
 * @param ctx the parse tree
 */
fn enter_hash_partitions(&mut self, _ctx: &Hash_partitionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hash_partitions}.
 * @param ctx the parse tree
 */
fn exit_hash_partitions(&mut self, _ctx: &Hash_partitionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#individual_hash_partitions}.
 * @param ctx the parse tree
 */
fn enter_individual_hash_partitions(&mut self, _ctx: &Individual_hash_partitionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#individual_hash_partitions}.
 * @param ctx the parse tree
 */
fn exit_individual_hash_partitions(&mut self, _ctx: &Individual_hash_partitionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hash_partitions_by_quantity}.
 * @param ctx the parse tree
 */
fn enter_hash_partitions_by_quantity(&mut self, _ctx: &Hash_partitions_by_quantityContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hash_partitions_by_quantity}.
 * @param ctx the parse tree
 */
fn exit_hash_partitions_by_quantity(&mut self, _ctx: &Hash_partitions_by_quantityContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hash_partition_quantity}.
 * @param ctx the parse tree
 */
fn enter_hash_partition_quantity(&mut self, _ctx: &Hash_partition_quantityContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hash_partition_quantity}.
 * @param ctx the parse tree
 */
fn exit_hash_partition_quantity(&mut self, _ctx: &Hash_partition_quantityContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#composite_range_partitions}.
 * @param ctx the parse tree
 */
fn enter_composite_range_partitions(&mut self, _ctx: &Composite_range_partitionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#composite_range_partitions}.
 * @param ctx the parse tree
 */
fn exit_composite_range_partitions(&mut self, _ctx: &Composite_range_partitionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#composite_list_partitions}.
 * @param ctx the parse tree
 */
fn enter_composite_list_partitions(&mut self, _ctx: &Composite_list_partitionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#composite_list_partitions}.
 * @param ctx the parse tree
 */
fn exit_composite_list_partitions(&mut self, _ctx: &Composite_list_partitionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#composite_hash_partitions}.
 * @param ctx the parse tree
 */
fn enter_composite_hash_partitions(&mut self, _ctx: &Composite_hash_partitionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#composite_hash_partitions}.
 * @param ctx the parse tree
 */
fn exit_composite_hash_partitions(&mut self, _ctx: &Composite_hash_partitionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#reference_partitioning}.
 * @param ctx the parse tree
 */
fn enter_reference_partitioning(&mut self, _ctx: &Reference_partitioningContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#reference_partitioning}.
 * @param ctx the parse tree
 */
fn exit_reference_partitioning(&mut self, _ctx: &Reference_partitioningContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#reference_partition_desc}.
 * @param ctx the parse tree
 */
fn enter_reference_partition_desc(&mut self, _ctx: &Reference_partition_descContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#reference_partition_desc}.
 * @param ctx the parse tree
 */
fn exit_reference_partition_desc(&mut self, _ctx: &Reference_partition_descContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#system_partitioning}.
 * @param ctx the parse tree
 */
fn enter_system_partitioning(&mut self, _ctx: &System_partitioningContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#system_partitioning}.
 * @param ctx the parse tree
 */
fn exit_system_partitioning(&mut self, _ctx: &System_partitioningContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#range_partition_desc}.
 * @param ctx the parse tree
 */
fn enter_range_partition_desc(&mut self, _ctx: &Range_partition_descContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#range_partition_desc}.
 * @param ctx the parse tree
 */
fn exit_range_partition_desc(&mut self, _ctx: &Range_partition_descContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#list_partition_desc}.
 * @param ctx the parse tree
 */
fn enter_list_partition_desc(&mut self, _ctx: &List_partition_descContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#list_partition_desc}.
 * @param ctx the parse tree
 */
fn exit_list_partition_desc(&mut self, _ctx: &List_partition_descContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subpartition_template}.
 * @param ctx the parse tree
 */
fn enter_subpartition_template(&mut self, _ctx: &Subpartition_templateContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subpartition_template}.
 * @param ctx the parse tree
 */
fn exit_subpartition_template(&mut self, _ctx: &Subpartition_templateContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hash_subpartition_quantity}.
 * @param ctx the parse tree
 */
fn enter_hash_subpartition_quantity(&mut self, _ctx: &Hash_subpartition_quantityContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hash_subpartition_quantity}.
 * @param ctx the parse tree
 */
fn exit_hash_subpartition_quantity(&mut self, _ctx: &Hash_subpartition_quantityContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subpartition_by_range}.
 * @param ctx the parse tree
 */
fn enter_subpartition_by_range(&mut self, _ctx: &Subpartition_by_rangeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subpartition_by_range}.
 * @param ctx the parse tree
 */
fn exit_subpartition_by_range(&mut self, _ctx: &Subpartition_by_rangeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subpartition_by_list}.
 * @param ctx the parse tree
 */
fn enter_subpartition_by_list(&mut self, _ctx: &Subpartition_by_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subpartition_by_list}.
 * @param ctx the parse tree
 */
fn exit_subpartition_by_list(&mut self, _ctx: &Subpartition_by_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subpartition_by_hash}.
 * @param ctx the parse tree
 */
fn enter_subpartition_by_hash(&mut self, _ctx: &Subpartition_by_hashContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subpartition_by_hash}.
 * @param ctx the parse tree
 */
fn exit_subpartition_by_hash(&mut self, _ctx: &Subpartition_by_hashContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subpartition_name}.
 * @param ctx the parse tree
 */
fn enter_subpartition_name(&mut self, _ctx: &Subpartition_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subpartition_name}.
 * @param ctx the parse tree
 */
fn exit_subpartition_name(&mut self, _ctx: &Subpartition_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#range_subpartition_desc}.
 * @param ctx the parse tree
 */
fn enter_range_subpartition_desc(&mut self, _ctx: &Range_subpartition_descContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#range_subpartition_desc}.
 * @param ctx the parse tree
 */
fn exit_range_subpartition_desc(&mut self, _ctx: &Range_subpartition_descContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#list_subpartition_desc}.
 * @param ctx the parse tree
 */
fn enter_list_subpartition_desc(&mut self, _ctx: &List_subpartition_descContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#list_subpartition_desc}.
 * @param ctx the parse tree
 */
fn exit_list_subpartition_desc(&mut self, _ctx: &List_subpartition_descContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#individual_hash_subparts}.
 * @param ctx the parse tree
 */
fn enter_individual_hash_subparts(&mut self, _ctx: &Individual_hash_subpartsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#individual_hash_subparts}.
 * @param ctx the parse tree
 */
fn exit_individual_hash_subparts(&mut self, _ctx: &Individual_hash_subpartsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hash_subparts_by_quantity}.
 * @param ctx the parse tree
 */
fn enter_hash_subparts_by_quantity(&mut self, _ctx: &Hash_subparts_by_quantityContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hash_subparts_by_quantity}.
 * @param ctx the parse tree
 */
fn exit_hash_subparts_by_quantity(&mut self, _ctx: &Hash_subparts_by_quantityContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#range_values_clause}.
 * @param ctx the parse tree
 */
fn enter_range_values_clause(&mut self, _ctx: &Range_values_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#range_values_clause}.
 * @param ctx the parse tree
 */
fn exit_range_values_clause(&mut self, _ctx: &Range_values_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#range_values_list}.
 * @param ctx the parse tree
 */
fn enter_range_values_list(&mut self, _ctx: &Range_values_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#range_values_list}.
 * @param ctx the parse tree
 */
fn exit_range_values_list(&mut self, _ctx: &Range_values_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#list_values_clause}.
 * @param ctx the parse tree
 */
fn enter_list_values_clause(&mut self, _ctx: &List_values_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#list_values_clause}.
 * @param ctx the parse tree
 */
fn exit_list_values_clause(&mut self, _ctx: &List_values_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_partition_description}.
 * @param ctx the parse tree
 */
fn enter_table_partition_description(&mut self, _ctx: &Table_partition_descriptionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_partition_description}.
 * @param ctx the parse tree
 */
fn exit_table_partition_description(&mut self, _ctx: &Table_partition_descriptionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partitioning_storage_clause}.
 * @param ctx the parse tree
 */
fn enter_partitioning_storage_clause(&mut self, _ctx: &Partitioning_storage_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partitioning_storage_clause}.
 * @param ctx the parse tree
 */
fn exit_partitioning_storage_clause(&mut self, _ctx: &Partitioning_storage_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_partitioning_storage}.
 * @param ctx the parse tree
 */
fn enter_lob_partitioning_storage(&mut self, _ctx: &Lob_partitioning_storageContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_partitioning_storage}.
 * @param ctx the parse tree
 */
fn exit_lob_partitioning_storage(&mut self, _ctx: &Lob_partitioning_storageContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#size_clause}.
 * @param ctx the parse tree
 */
fn enter_size_clause(&mut self, _ctx: &Size_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#size_clause}.
 * @param ctx the parse tree
 */
fn exit_size_clause(&mut self, _ctx: &Size_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_compression}.
 * @param ctx the parse tree
 */
fn enter_table_compression(&mut self, _ctx: &Table_compressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_compression}.
 * @param ctx the parse tree
 */
fn exit_table_compression(&mut self, _ctx: &Table_compressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inmemory_table_clause}.
 * @param ctx the parse tree
 */
fn enter_inmemory_table_clause(&mut self, _ctx: &Inmemory_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inmemory_table_clause}.
 * @param ctx the parse tree
 */
fn exit_inmemory_table_clause(&mut self, _ctx: &Inmemory_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inmemory_attributes}.
 * @param ctx the parse tree
 */
fn enter_inmemory_attributes(&mut self, _ctx: &Inmemory_attributesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inmemory_attributes}.
 * @param ctx the parse tree
 */
fn exit_inmemory_attributes(&mut self, _ctx: &Inmemory_attributesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inmemory_memcompress}.
 * @param ctx the parse tree
 */
fn enter_inmemory_memcompress(&mut self, _ctx: &Inmemory_memcompressContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inmemory_memcompress}.
 * @param ctx the parse tree
 */
fn exit_inmemory_memcompress(&mut self, _ctx: &Inmemory_memcompressContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inmemory_priority}.
 * @param ctx the parse tree
 */
fn enter_inmemory_priority(&mut self, _ctx: &Inmemory_priorityContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inmemory_priority}.
 * @param ctx the parse tree
 */
fn exit_inmemory_priority(&mut self, _ctx: &Inmemory_priorityContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inmemory_distribute}.
 * @param ctx the parse tree
 */
fn enter_inmemory_distribute(&mut self, _ctx: &Inmemory_distributeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inmemory_distribute}.
 * @param ctx the parse tree
 */
fn exit_inmemory_distribute(&mut self, _ctx: &Inmemory_distributeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inmemory_duplicate}.
 * @param ctx the parse tree
 */
fn enter_inmemory_duplicate(&mut self, _ctx: &Inmemory_duplicateContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inmemory_duplicate}.
 * @param ctx the parse tree
 */
fn exit_inmemory_duplicate(&mut self, _ctx: &Inmemory_duplicateContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inmemory_column_clause}.
 * @param ctx the parse tree
 */
fn enter_inmemory_column_clause(&mut self, _ctx: &Inmemory_column_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inmemory_column_clause}.
 * @param ctx the parse tree
 */
fn exit_inmemory_column_clause(&mut self, _ctx: &Inmemory_column_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#physical_attributes_clause}.
 * @param ctx the parse tree
 */
fn enter_physical_attributes_clause(&mut self, _ctx: &Physical_attributes_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#physical_attributes_clause}.
 * @param ctx the parse tree
 */
fn exit_physical_attributes_clause(&mut self, _ctx: &Physical_attributes_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#storage_clause}.
 * @param ctx the parse tree
 */
fn enter_storage_clause(&mut self, _ctx: &Storage_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#storage_clause}.
 * @param ctx the parse tree
 */
fn exit_storage_clause(&mut self, _ctx: &Storage_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#deferred_segment_creation}.
 * @param ctx the parse tree
 */
fn enter_deferred_segment_creation(&mut self, _ctx: &Deferred_segment_creationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#deferred_segment_creation}.
 * @param ctx the parse tree
 */
fn exit_deferred_segment_creation(&mut self, _ctx: &Deferred_segment_creationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#segment_attributes_clause}.
 * @param ctx the parse tree
 */
fn enter_segment_attributes_clause(&mut self, _ctx: &Segment_attributes_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#segment_attributes_clause}.
 * @param ctx the parse tree
 */
fn exit_segment_attributes_clause(&mut self, _ctx: &Segment_attributes_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#physical_properties}.
 * @param ctx the parse tree
 */
fn enter_physical_properties(&mut self, _ctx: &Physical_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#physical_properties}.
 * @param ctx the parse tree
 */
fn exit_physical_properties(&mut self, _ctx: &Physical_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ilm_clause}.
 * @param ctx the parse tree
 */
fn enter_ilm_clause(&mut self, _ctx: &Ilm_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ilm_clause}.
 * @param ctx the parse tree
 */
fn exit_ilm_clause(&mut self, _ctx: &Ilm_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ilm_policy_clause}.
 * @param ctx the parse tree
 */
fn enter_ilm_policy_clause(&mut self, _ctx: &Ilm_policy_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ilm_policy_clause}.
 * @param ctx the parse tree
 */
fn exit_ilm_policy_clause(&mut self, _ctx: &Ilm_policy_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ilm_compression_policy}.
 * @param ctx the parse tree
 */
fn enter_ilm_compression_policy(&mut self, _ctx: &Ilm_compression_policyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ilm_compression_policy}.
 * @param ctx the parse tree
 */
fn exit_ilm_compression_policy(&mut self, _ctx: &Ilm_compression_policyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ilm_tiering_policy}.
 * @param ctx the parse tree
 */
fn enter_ilm_tiering_policy(&mut self, _ctx: &Ilm_tiering_policyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ilm_tiering_policy}.
 * @param ctx the parse tree
 */
fn exit_ilm_tiering_policy(&mut self, _ctx: &Ilm_tiering_policyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ilm_after_on}.
 * @param ctx the parse tree
 */
fn enter_ilm_after_on(&mut self, _ctx: &Ilm_after_onContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ilm_after_on}.
 * @param ctx the parse tree
 */
fn exit_ilm_after_on(&mut self, _ctx: &Ilm_after_onContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#segment_group}.
 * @param ctx the parse tree
 */
fn enter_segment_group(&mut self, _ctx: &Segment_groupContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#segment_group}.
 * @param ctx the parse tree
 */
fn exit_segment_group(&mut self, _ctx: &Segment_groupContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ilm_inmemory_policy}.
 * @param ctx the parse tree
 */
fn enter_ilm_inmemory_policy(&mut self, _ctx: &Ilm_inmemory_policyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ilm_inmemory_policy}.
 * @param ctx the parse tree
 */
fn exit_ilm_inmemory_policy(&mut self, _ctx: &Ilm_inmemory_policyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ilm_time_period}.
 * @param ctx the parse tree
 */
fn enter_ilm_time_period(&mut self, _ctx: &Ilm_time_periodContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ilm_time_period}.
 * @param ctx the parse tree
 */
fn exit_ilm_time_period(&mut self, _ctx: &Ilm_time_periodContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#heap_org_table_clause}.
 * @param ctx the parse tree
 */
fn enter_heap_org_table_clause(&mut self, _ctx: &Heap_org_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#heap_org_table_clause}.
 * @param ctx the parse tree
 */
fn exit_heap_org_table_clause(&mut self, _ctx: &Heap_org_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_clause(&mut self, _ctx: &External_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_clause(&mut self, _ctx: &External_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#access_driver_type}.
 * @param ctx the parse tree
 */
fn enter_access_driver_type(&mut self, _ctx: &Access_driver_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#access_driver_type}.
 * @param ctx the parse tree
 */
fn exit_access_driver_type(&mut self, _ctx: &Access_driver_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_data_props}.
 * @param ctx the parse tree
 */
fn enter_external_table_data_props(&mut self, _ctx: &External_table_data_propsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_data_props}.
 * @param ctx the parse tree
 */
fn exit_external_table_data_props(&mut self, _ctx: &External_table_data_propsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_data_format}.
 * @param ctx the parse tree
 */
fn enter_external_table_data_format(&mut self, _ctx: &External_table_data_formatContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_data_format}.
 * @param ctx the parse tree
 */
fn exit_external_table_data_format(&mut self, _ctx: &External_table_data_formatContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_transform}.
 * @param ctx the parse tree
 */
fn enter_external_table_transform(&mut self, _ctx: &External_table_transformContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_transform}.
 * @param ctx the parse tree
 */
fn exit_external_table_transform(&mut self, _ctx: &External_table_transformContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_field}.
 * @param ctx the parse tree
 */
fn enter_external_table_field(&mut self, _ctx: &External_table_fieldContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_field}.
 * @param ctx the parse tree
 */
fn exit_external_table_field(&mut self, _ctx: &External_table_fieldContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_field_list}.
 * @param ctx the parse tree
 */
fn enter_external_table_field_list(&mut self, _ctx: &External_table_field_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_field_list}.
 * @param ctx the parse tree
 */
fn exit_external_table_field_list(&mut self, _ctx: &External_table_field_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_fields_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_fields_clause(&mut self, _ctx: &External_table_fields_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_fields_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_fields_clause(&mut self, _ctx: &External_table_fields_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_position_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_position_clause(&mut self, _ctx: &External_table_position_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_position_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_position_clause(&mut self, _ctx: &External_table_position_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_datatype_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_datatype_clause(&mut self, _ctx: &External_table_datatype_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_datatype_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_datatype_clause(&mut self, _ctx: &External_table_datatype_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_delimit_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_delimit_clause(&mut self, _ctx: &External_table_delimit_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_delimit_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_delimit_clause(&mut self, _ctx: &External_table_delimit_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_trim_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_trim_clause(&mut self, _ctx: &External_table_trim_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_trim_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_trim_clause(&mut self, _ctx: &External_table_trim_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_date_format_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_date_format_clause(&mut self, _ctx: &External_table_date_format_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_date_format_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_date_format_clause(&mut self, _ctx: &External_table_date_format_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_init_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_init_clause(&mut self, _ctx: &External_table_init_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_init_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_init_clause(&mut self, _ctx: &External_table_init_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_condition_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_condition_clause(&mut self, _ctx: &External_table_condition_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_condition_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_condition_clause(&mut self, _ctx: &External_table_condition_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_lls_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_lls_clause(&mut self, _ctx: &External_table_lls_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_lls_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_lls_clause(&mut self, _ctx: &External_table_lls_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_records}.
 * @param ctx the parse tree
 */
fn enter_external_table_records(&mut self, _ctx: &External_table_recordsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_records}.
 * @param ctx the parse tree
 */
fn exit_external_table_records(&mut self, _ctx: &External_table_recordsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_record_options_clause}.
 * @param ctx the parse tree
 */
fn enter_external_table_record_options_clause(&mut self, _ctx: &External_table_record_options_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_record_options_clause}.
 * @param ctx the parse tree
 */
fn exit_external_table_record_options_clause(&mut self, _ctx: &External_table_record_options_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_output_files}.
 * @param ctx the parse tree
 */
fn enter_external_table_output_files(&mut self, _ctx: &External_table_output_filesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_output_files}.
 * @param ctx the parse tree
 */
fn exit_external_table_output_files(&mut self, _ctx: &External_table_output_filesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_fields}.
 * @param ctx the parse tree
 */
fn enter_external_table_fields(&mut self, _ctx: &External_table_fieldsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_fields}.
 * @param ctx the parse tree
 */
fn exit_external_table_fields(&mut self, _ctx: &External_table_fieldsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_datapump}.
 * @param ctx the parse tree
 */
fn enter_external_table_datapump(&mut self, _ctx: &External_table_datapumpContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_datapump}.
 * @param ctx the parse tree
 */
fn exit_external_table_datapump(&mut self, _ctx: &External_table_datapumpContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_hive}.
 * @param ctx the parse tree
 */
fn enter_external_table_hive(&mut self, _ctx: &External_table_hiveContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_hive}.
 * @param ctx the parse tree
 */
fn exit_external_table_hive(&mut self, _ctx: &External_table_hiveContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_hive_parameter_map}.
 * @param ctx the parse tree
 */
fn enter_external_table_hive_parameter_map(&mut self, _ctx: &External_table_hive_parameter_mapContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_hive_parameter_map}.
 * @param ctx the parse tree
 */
fn exit_external_table_hive_parameter_map(&mut self, _ctx: &External_table_hive_parameter_mapContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_hive_parameter_map_entry}.
 * @param ctx the parse tree
 */
fn enter_external_table_hive_parameter_map_entry(&mut self, _ctx: &External_table_hive_parameter_map_entryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_hive_parameter_map_entry}.
 * @param ctx the parse tree
 */
fn exit_external_table_hive_parameter_map_entry(&mut self, _ctx: &External_table_hive_parameter_map_entryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#external_table_directory}.
 * @param ctx the parse tree
 */
fn enter_external_table_directory(&mut self, _ctx: &External_table_directoryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#external_table_directory}.
 * @param ctx the parse tree
 */
fn exit_external_table_directory(&mut self, _ctx: &External_table_directoryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#row_movement_clause}.
 * @param ctx the parse tree
 */
fn enter_row_movement_clause(&mut self, _ctx: &Row_movement_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#row_movement_clause}.
 * @param ctx the parse tree
 */
fn exit_row_movement_clause(&mut self, _ctx: &Row_movement_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#flashback_archive_clause}.
 * @param ctx the parse tree
 */
fn enter_flashback_archive_clause(&mut self, _ctx: &Flashback_archive_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#flashback_archive_clause}.
 * @param ctx the parse tree
 */
fn exit_flashback_archive_clause(&mut self, _ctx: &Flashback_archive_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#log_grp}.
 * @param ctx the parse tree
 */
fn enter_log_grp(&mut self, _ctx: &Log_grpContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#log_grp}.
 * @param ctx the parse tree
 */
fn exit_log_grp(&mut self, _ctx: &Log_grpContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#supplemental_table_logging}.
 * @param ctx the parse tree
 */
fn enter_supplemental_table_logging(&mut self, _ctx: &Supplemental_table_loggingContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#supplemental_table_logging}.
 * @param ctx the parse tree
 */
fn exit_supplemental_table_logging(&mut self, _ctx: &Supplemental_table_loggingContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#supplemental_log_grp_clause}.
 * @param ctx the parse tree
 */
fn enter_supplemental_log_grp_clause(&mut self, _ctx: &Supplemental_log_grp_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#supplemental_log_grp_clause}.
 * @param ctx the parse tree
 */
fn exit_supplemental_log_grp_clause(&mut self, _ctx: &Supplemental_log_grp_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#supplemental_id_key_clause}.
 * @param ctx the parse tree
 */
fn enter_supplemental_id_key_clause(&mut self, _ctx: &Supplemental_id_key_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#supplemental_id_key_clause}.
 * @param ctx the parse tree
 */
fn exit_supplemental_id_key_clause(&mut self, _ctx: &Supplemental_id_key_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#allocate_extent_clause}.
 * @param ctx the parse tree
 */
fn enter_allocate_extent_clause(&mut self, _ctx: &Allocate_extent_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#allocate_extent_clause}.
 * @param ctx the parse tree
 */
fn exit_allocate_extent_clause(&mut self, _ctx: &Allocate_extent_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#deallocate_unused_clause}.
 * @param ctx the parse tree
 */
fn enter_deallocate_unused_clause(&mut self, _ctx: &Deallocate_unused_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#deallocate_unused_clause}.
 * @param ctx the parse tree
 */
fn exit_deallocate_unused_clause(&mut self, _ctx: &Deallocate_unused_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#shrink_clause}.
 * @param ctx the parse tree
 */
fn enter_shrink_clause(&mut self, _ctx: &Shrink_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#shrink_clause}.
 * @param ctx the parse tree
 */
fn exit_shrink_clause(&mut self, _ctx: &Shrink_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#records_per_block_clause}.
 * @param ctx the parse tree
 */
fn enter_records_per_block_clause(&mut self, _ctx: &Records_per_block_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#records_per_block_clause}.
 * @param ctx the parse tree
 */
fn exit_records_per_block_clause(&mut self, _ctx: &Records_per_block_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#upgrade_table_clause}.
 * @param ctx the parse tree
 */
fn enter_upgrade_table_clause(&mut self, _ctx: &Upgrade_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#upgrade_table_clause}.
 * @param ctx the parse tree
 */
fn exit_upgrade_table_clause(&mut self, _ctx: &Upgrade_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#truncate_table}.
 * @param ctx the parse tree
 */
fn enter_truncate_table(&mut self, _ctx: &Truncate_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#truncate_table}.
 * @param ctx the parse tree
 */
fn exit_truncate_table(&mut self, _ctx: &Truncate_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_table}.
 * @param ctx the parse tree
 */
fn enter_drop_table(&mut self, _ctx: &Drop_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_table}.
 * @param ctx the parse tree
 */
fn exit_drop_table(&mut self, _ctx: &Drop_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_tablespace}.
 * @param ctx the parse tree
 */
fn enter_drop_tablespace(&mut self, _ctx: &Drop_tablespaceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_tablespace}.
 * @param ctx the parse tree
 */
fn exit_drop_tablespace(&mut self, _ctx: &Drop_tablespaceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_tablespace_set}.
 * @param ctx the parse tree
 */
fn enter_drop_tablespace_set(&mut self, _ctx: &Drop_tablespace_setContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_tablespace_set}.
 * @param ctx the parse tree
 */
fn exit_drop_tablespace_set(&mut self, _ctx: &Drop_tablespace_setContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#including_contents_clause}.
 * @param ctx the parse tree
 */
fn enter_including_contents_clause(&mut self, _ctx: &Including_contents_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#including_contents_clause}.
 * @param ctx the parse tree
 */
fn exit_including_contents_clause(&mut self, _ctx: &Including_contents_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_view}.
 * @param ctx the parse tree
 */
fn enter_drop_view(&mut self, _ctx: &Drop_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_view}.
 * @param ctx the parse tree
 */
fn exit_drop_view(&mut self, _ctx: &Drop_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#comment_on_column}.
 * @param ctx the parse tree
 */
fn enter_comment_on_column(&mut self, _ctx: &Comment_on_columnContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#comment_on_column}.
 * @param ctx the parse tree
 */
fn exit_comment_on_column(&mut self, _ctx: &Comment_on_columnContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#enable_or_disable}.
 * @param ctx the parse tree
 */
fn enter_enable_or_disable(&mut self, _ctx: &Enable_or_disableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#enable_or_disable}.
 * @param ctx the parse tree
 */
fn exit_enable_or_disable(&mut self, _ctx: &Enable_or_disableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#allow_or_disallow}.
 * @param ctx the parse tree
 */
fn enter_allow_or_disallow(&mut self, _ctx: &Allow_or_disallowContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#allow_or_disallow}.
 * @param ctx the parse tree
 */
fn exit_allow_or_disallow(&mut self, _ctx: &Allow_or_disallowContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_synonym}.
 * @param ctx the parse tree
 */
fn enter_alter_synonym(&mut self, _ctx: &Alter_synonymContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_synonym}.
 * @param ctx the parse tree
 */
fn exit_alter_synonym(&mut self, _ctx: &Alter_synonymContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_synonym}.
 * @param ctx the parse tree
 */
fn enter_create_synonym(&mut self, _ctx: &Create_synonymContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_synonym}.
 * @param ctx the parse tree
 */
fn exit_create_synonym(&mut self, _ctx: &Create_synonymContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_synonym}.
 * @param ctx the parse tree
 */
fn enter_drop_synonym(&mut self, _ctx: &Drop_synonymContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_synonym}.
 * @param ctx the parse tree
 */
fn exit_drop_synonym(&mut self, _ctx: &Drop_synonymContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_spfile}.
 * @param ctx the parse tree
 */
fn enter_create_spfile(&mut self, _ctx: &Create_spfileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_spfile}.
 * @param ctx the parse tree
 */
fn exit_create_spfile(&mut self, _ctx: &Create_spfileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#spfile_name}.
 * @param ctx the parse tree
 */
fn enter_spfile_name(&mut self, _ctx: &Spfile_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#spfile_name}.
 * @param ctx the parse tree
 */
fn exit_spfile_name(&mut self, _ctx: &Spfile_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pfile_name}.
 * @param ctx the parse tree
 */
fn enter_pfile_name(&mut self, _ctx: &Pfile_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pfile_name}.
 * @param ctx the parse tree
 */
fn exit_pfile_name(&mut self, _ctx: &Pfile_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#comment_on_table}.
 * @param ctx the parse tree
 */
fn enter_comment_on_table(&mut self, _ctx: &Comment_on_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#comment_on_table}.
 * @param ctx the parse tree
 */
fn exit_comment_on_table(&mut self, _ctx: &Comment_on_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#comment_on_materialized}.
 * @param ctx the parse tree
 */
fn enter_comment_on_materialized(&mut self, _ctx: &Comment_on_materializedContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#comment_on_materialized}.
 * @param ctx the parse tree
 */
fn exit_comment_on_materialized(&mut self, _ctx: &Comment_on_materializedContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_analytic_view}.
 * @param ctx the parse tree
 */
fn enter_alter_analytic_view(&mut self, _ctx: &Alter_analytic_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_analytic_view}.
 * @param ctx the parse tree
 */
fn exit_alter_analytic_view(&mut self, _ctx: &Alter_analytic_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_add_cache_clause}.
 * @param ctx the parse tree
 */
fn enter_alter_add_cache_clause(&mut self, _ctx: &Alter_add_cache_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_add_cache_clause}.
 * @param ctx the parse tree
 */
fn exit_alter_add_cache_clause(&mut self, _ctx: &Alter_add_cache_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#levels_item}.
 * @param ctx the parse tree
 */
fn enter_levels_item(&mut self, _ctx: &Levels_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#levels_item}.
 * @param ctx the parse tree
 */
fn exit_levels_item(&mut self, _ctx: &Levels_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#measure_list}.
 * @param ctx the parse tree
 */
fn enter_measure_list(&mut self, _ctx: &Measure_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#measure_list}.
 * @param ctx the parse tree
 */
fn exit_measure_list(&mut self, _ctx: &Measure_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_drop_cache_clause}.
 * @param ctx the parse tree
 */
fn enter_alter_drop_cache_clause(&mut self, _ctx: &Alter_drop_cache_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_drop_cache_clause}.
 * @param ctx the parse tree
 */
fn exit_alter_drop_cache_clause(&mut self, _ctx: &Alter_drop_cache_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_attribute_dimension}.
 * @param ctx the parse tree
 */
fn enter_alter_attribute_dimension(&mut self, _ctx: &Alter_attribute_dimensionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_attribute_dimension}.
 * @param ctx the parse tree
 */
fn exit_alter_attribute_dimension(&mut self, _ctx: &Alter_attribute_dimensionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_audit_policy}.
 * @param ctx the parse tree
 */
fn enter_alter_audit_policy(&mut self, _ctx: &Alter_audit_policyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_audit_policy}.
 * @param ctx the parse tree
 */
fn exit_alter_audit_policy(&mut self, _ctx: &Alter_audit_policyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_cluster}.
 * @param ctx the parse tree
 */
fn enter_alter_cluster(&mut self, _ctx: &Alter_clusterContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_cluster}.
 * @param ctx the parse tree
 */
fn exit_alter_cluster(&mut self, _ctx: &Alter_clusterContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_analytic_view}.
 * @param ctx the parse tree
 */
fn enter_drop_analytic_view(&mut self, _ctx: &Drop_analytic_viewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_analytic_view}.
 * @param ctx the parse tree
 */
fn exit_drop_analytic_view(&mut self, _ctx: &Drop_analytic_viewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_attribute_dimension}.
 * @param ctx the parse tree
 */
fn enter_drop_attribute_dimension(&mut self, _ctx: &Drop_attribute_dimensionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_attribute_dimension}.
 * @param ctx the parse tree
 */
fn exit_drop_attribute_dimension(&mut self, _ctx: &Drop_attribute_dimensionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_audit_policy}.
 * @param ctx the parse tree
 */
fn enter_drop_audit_policy(&mut self, _ctx: &Drop_audit_policyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_audit_policy}.
 * @param ctx the parse tree
 */
fn exit_drop_audit_policy(&mut self, _ctx: &Drop_audit_policyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_flashback_archive}.
 * @param ctx the parse tree
 */
fn enter_drop_flashback_archive(&mut self, _ctx: &Drop_flashback_archiveContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_flashback_archive}.
 * @param ctx the parse tree
 */
fn exit_drop_flashback_archive(&mut self, _ctx: &Drop_flashback_archiveContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_cluster}.
 * @param ctx the parse tree
 */
fn enter_drop_cluster(&mut self, _ctx: &Drop_clusterContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_cluster}.
 * @param ctx the parse tree
 */
fn exit_drop_cluster(&mut self, _ctx: &Drop_clusterContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_context}.
 * @param ctx the parse tree
 */
fn enter_drop_context(&mut self, _ctx: &Drop_contextContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_context}.
 * @param ctx the parse tree
 */
fn exit_drop_context(&mut self, _ctx: &Drop_contextContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_directory}.
 * @param ctx the parse tree
 */
fn enter_drop_directory(&mut self, _ctx: &Drop_directoryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_directory}.
 * @param ctx the parse tree
 */
fn exit_drop_directory(&mut self, _ctx: &Drop_directoryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_diskgroup}.
 * @param ctx the parse tree
 */
fn enter_drop_diskgroup(&mut self, _ctx: &Drop_diskgroupContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_diskgroup}.
 * @param ctx the parse tree
 */
fn exit_drop_diskgroup(&mut self, _ctx: &Drop_diskgroupContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_edition}.
 * @param ctx the parse tree
 */
fn enter_drop_edition(&mut self, _ctx: &Drop_editionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_edition}.
 * @param ctx the parse tree
 */
fn exit_drop_edition(&mut self, _ctx: &Drop_editionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#truncate_cluster}.
 * @param ctx the parse tree
 */
fn enter_truncate_cluster(&mut self, _ctx: &Truncate_clusterContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#truncate_cluster}.
 * @param ctx the parse tree
 */
fn exit_truncate_cluster(&mut self, _ctx: &Truncate_clusterContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cache_or_nocache}.
 * @param ctx the parse tree
 */
fn enter_cache_or_nocache(&mut self, _ctx: &Cache_or_nocacheContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cache_or_nocache}.
 * @param ctx the parse tree
 */
fn exit_cache_or_nocache(&mut self, _ctx: &Cache_or_nocacheContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#database_name}.
 * @param ctx the parse tree
 */
fn enter_database_name(&mut self, _ctx: &Database_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#database_name}.
 * @param ctx the parse tree
 */
fn exit_database_name(&mut self, _ctx: &Database_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_database}.
 * @param ctx the parse tree
 */
fn enter_alter_database(&mut self, _ctx: &Alter_databaseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_database}.
 * @param ctx the parse tree
 */
fn exit_alter_database(&mut self, _ctx: &Alter_databaseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#database_clause}.
 * @param ctx the parse tree
 */
fn enter_database_clause(&mut self, _ctx: &Database_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#database_clause}.
 * @param ctx the parse tree
 */
fn exit_database_clause(&mut self, _ctx: &Database_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#startup_clauses}.
 * @param ctx the parse tree
 */
fn enter_startup_clauses(&mut self, _ctx: &Startup_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#startup_clauses}.
 * @param ctx the parse tree
 */
fn exit_startup_clauses(&mut self, _ctx: &Startup_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#resetlogs_or_noresetlogs}.
 * @param ctx the parse tree
 */
fn enter_resetlogs_or_noresetlogs(&mut self, _ctx: &Resetlogs_or_noresetlogsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#resetlogs_or_noresetlogs}.
 * @param ctx the parse tree
 */
fn exit_resetlogs_or_noresetlogs(&mut self, _ctx: &Resetlogs_or_noresetlogsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#upgrade_or_downgrade}.
 * @param ctx the parse tree
 */
fn enter_upgrade_or_downgrade(&mut self, _ctx: &Upgrade_or_downgradeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#upgrade_or_downgrade}.
 * @param ctx the parse tree
 */
fn exit_upgrade_or_downgrade(&mut self, _ctx: &Upgrade_or_downgradeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#recovery_clauses}.
 * @param ctx the parse tree
 */
fn enter_recovery_clauses(&mut self, _ctx: &Recovery_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#recovery_clauses}.
 * @param ctx the parse tree
 */
fn exit_recovery_clauses(&mut self, _ctx: &Recovery_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#begin_or_end}.
 * @param ctx the parse tree
 */
fn enter_begin_or_end(&mut self, _ctx: &Begin_or_endContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#begin_or_end}.
 * @param ctx the parse tree
 */
fn exit_begin_or_end(&mut self, _ctx: &Begin_or_endContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#general_recovery}.
 * @param ctx the parse tree
 */
fn enter_general_recovery(&mut self, _ctx: &General_recoveryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#general_recovery}.
 * @param ctx the parse tree
 */
fn exit_general_recovery(&mut self, _ctx: &General_recoveryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#full_database_recovery}.
 * @param ctx the parse tree
 */
fn enter_full_database_recovery(&mut self, _ctx: &Full_database_recoveryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#full_database_recovery}.
 * @param ctx the parse tree
 */
fn exit_full_database_recovery(&mut self, _ctx: &Full_database_recoveryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partial_database_recovery}.
 * @param ctx the parse tree
 */
fn enter_partial_database_recovery(&mut self, _ctx: &Partial_database_recoveryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partial_database_recovery}.
 * @param ctx the parse tree
 */
fn exit_partial_database_recovery(&mut self, _ctx: &Partial_database_recoveryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partial_database_recovery_10g}.
 * @param ctx the parse tree
 */
fn enter_partial_database_recovery_10g(&mut self, _ctx: &Partial_database_recovery_10gContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partial_database_recovery_10g}.
 * @param ctx the parse tree
 */
fn exit_partial_database_recovery_10g(&mut self, _ctx: &Partial_database_recovery_10gContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#managed_standby_recovery}.
 * @param ctx the parse tree
 */
fn enter_managed_standby_recovery(&mut self, _ctx: &Managed_standby_recoveryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#managed_standby_recovery}.
 * @param ctx the parse tree
 */
fn exit_managed_standby_recovery(&mut self, _ctx: &Managed_standby_recoveryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#db_name}.
 * @param ctx the parse tree
 */
fn enter_db_name(&mut self, _ctx: &Db_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#db_name}.
 * @param ctx the parse tree
 */
fn exit_db_name(&mut self, _ctx: &Db_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#database_file_clauses}.
 * @param ctx the parse tree
 */
fn enter_database_file_clauses(&mut self, _ctx: &Database_file_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#database_file_clauses}.
 * @param ctx the parse tree
 */
fn exit_database_file_clauses(&mut self, _ctx: &Database_file_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_datafile_clause}.
 * @param ctx the parse tree
 */
fn enter_create_datafile_clause(&mut self, _ctx: &Create_datafile_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_datafile_clause}.
 * @param ctx the parse tree
 */
fn exit_create_datafile_clause(&mut self, _ctx: &Create_datafile_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_datafile_clause}.
 * @param ctx the parse tree
 */
fn enter_alter_datafile_clause(&mut self, _ctx: &Alter_datafile_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_datafile_clause}.
 * @param ctx the parse tree
 */
fn exit_alter_datafile_clause(&mut self, _ctx: &Alter_datafile_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_tempfile_clause}.
 * @param ctx the parse tree
 */
fn enter_alter_tempfile_clause(&mut self, _ctx: &Alter_tempfile_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_tempfile_clause}.
 * @param ctx the parse tree
 */
fn exit_alter_tempfile_clause(&mut self, _ctx: &Alter_tempfile_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#move_datafile_clause}.
 * @param ctx the parse tree
 */
fn enter_move_datafile_clause(&mut self, _ctx: &Move_datafile_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#move_datafile_clause}.
 * @param ctx the parse tree
 */
fn exit_move_datafile_clause(&mut self, _ctx: &Move_datafile_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#logfile_clauses}.
 * @param ctx the parse tree
 */
fn enter_logfile_clauses(&mut self, _ctx: &Logfile_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#logfile_clauses}.
 * @param ctx the parse tree
 */
fn exit_logfile_clauses(&mut self, _ctx: &Logfile_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_logfile_clauses}.
 * @param ctx the parse tree
 */
fn enter_add_logfile_clauses(&mut self, _ctx: &Add_logfile_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_logfile_clauses}.
 * @param ctx the parse tree
 */
fn exit_add_logfile_clauses(&mut self, _ctx: &Add_logfile_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#group_redo_logfile}.
 * @param ctx the parse tree
 */
fn enter_group_redo_logfile(&mut self, _ctx: &Group_redo_logfileContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#group_redo_logfile}.
 * @param ctx the parse tree
 */
fn exit_group_redo_logfile(&mut self, _ctx: &Group_redo_logfileContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_logfile_clauses}.
 * @param ctx the parse tree
 */
fn enter_drop_logfile_clauses(&mut self, _ctx: &Drop_logfile_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_logfile_clauses}.
 * @param ctx the parse tree
 */
fn exit_drop_logfile_clauses(&mut self, _ctx: &Drop_logfile_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#switch_logfile_clause}.
 * @param ctx the parse tree
 */
fn enter_switch_logfile_clause(&mut self, _ctx: &Switch_logfile_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#switch_logfile_clause}.
 * @param ctx the parse tree
 */
fn exit_switch_logfile_clause(&mut self, _ctx: &Switch_logfile_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#supplemental_db_logging}.
 * @param ctx the parse tree
 */
fn enter_supplemental_db_logging(&mut self, _ctx: &Supplemental_db_loggingContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#supplemental_db_logging}.
 * @param ctx the parse tree
 */
fn exit_supplemental_db_logging(&mut self, _ctx: &Supplemental_db_loggingContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_or_drop}.
 * @param ctx the parse tree
 */
fn enter_add_or_drop(&mut self, _ctx: &Add_or_dropContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_or_drop}.
 * @param ctx the parse tree
 */
fn exit_add_or_drop(&mut self, _ctx: &Add_or_dropContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#supplemental_plsql_clause}.
 * @param ctx the parse tree
 */
fn enter_supplemental_plsql_clause(&mut self, _ctx: &Supplemental_plsql_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#supplemental_plsql_clause}.
 * @param ctx the parse tree
 */
fn exit_supplemental_plsql_clause(&mut self, _ctx: &Supplemental_plsql_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#logfile_descriptor}.
 * @param ctx the parse tree
 */
fn enter_logfile_descriptor(&mut self, _ctx: &Logfile_descriptorContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#logfile_descriptor}.
 * @param ctx the parse tree
 */
fn exit_logfile_descriptor(&mut self, _ctx: &Logfile_descriptorContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#controlfile_clauses}.
 * @param ctx the parse tree
 */
fn enter_controlfile_clauses(&mut self, _ctx: &Controlfile_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#controlfile_clauses}.
 * @param ctx the parse tree
 */
fn exit_controlfile_clauses(&mut self, _ctx: &Controlfile_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#trace_file_clause}.
 * @param ctx the parse tree
 */
fn enter_trace_file_clause(&mut self, _ctx: &Trace_file_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#trace_file_clause}.
 * @param ctx the parse tree
 */
fn exit_trace_file_clause(&mut self, _ctx: &Trace_file_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#standby_database_clauses}.
 * @param ctx the parse tree
 */
fn enter_standby_database_clauses(&mut self, _ctx: &Standby_database_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#standby_database_clauses}.
 * @param ctx the parse tree
 */
fn exit_standby_database_clauses(&mut self, _ctx: &Standby_database_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#activate_standby_db_clause}.
 * @param ctx the parse tree
 */
fn enter_activate_standby_db_clause(&mut self, _ctx: &Activate_standby_db_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#activate_standby_db_clause}.
 * @param ctx the parse tree
 */
fn exit_activate_standby_db_clause(&mut self, _ctx: &Activate_standby_db_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#maximize_standby_db_clause}.
 * @param ctx the parse tree
 */
fn enter_maximize_standby_db_clause(&mut self, _ctx: &Maximize_standby_db_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#maximize_standby_db_clause}.
 * @param ctx the parse tree
 */
fn exit_maximize_standby_db_clause(&mut self, _ctx: &Maximize_standby_db_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#register_logfile_clause}.
 * @param ctx the parse tree
 */
fn enter_register_logfile_clause(&mut self, _ctx: &Register_logfile_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#register_logfile_clause}.
 * @param ctx the parse tree
 */
fn exit_register_logfile_clause(&mut self, _ctx: &Register_logfile_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#commit_switchover_clause}.
 * @param ctx the parse tree
 */
fn enter_commit_switchover_clause(&mut self, _ctx: &Commit_switchover_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#commit_switchover_clause}.
 * @param ctx the parse tree
 */
fn exit_commit_switchover_clause(&mut self, _ctx: &Commit_switchover_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#start_standby_clause}.
 * @param ctx the parse tree
 */
fn enter_start_standby_clause(&mut self, _ctx: &Start_standby_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#start_standby_clause}.
 * @param ctx the parse tree
 */
fn exit_start_standby_clause(&mut self, _ctx: &Start_standby_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#stop_standby_clause}.
 * @param ctx the parse tree
 */
fn enter_stop_standby_clause(&mut self, _ctx: &Stop_standby_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#stop_standby_clause}.
 * @param ctx the parse tree
 */
fn exit_stop_standby_clause(&mut self, _ctx: &Stop_standby_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#convert_database_clause}.
 * @param ctx the parse tree
 */
fn enter_convert_database_clause(&mut self, _ctx: &Convert_database_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#convert_database_clause}.
 * @param ctx the parse tree
 */
fn exit_convert_database_clause(&mut self, _ctx: &Convert_database_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_settings_clause}.
 * @param ctx the parse tree
 */
fn enter_default_settings_clause(&mut self, _ctx: &Default_settings_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_settings_clause}.
 * @param ctx the parse tree
 */
fn exit_default_settings_clause(&mut self, _ctx: &Default_settings_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#set_time_zone_clause}.
 * @param ctx the parse tree
 */
fn enter_set_time_zone_clause(&mut self, _ctx: &Set_time_zone_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#set_time_zone_clause}.
 * @param ctx the parse tree
 */
fn exit_set_time_zone_clause(&mut self, _ctx: &Set_time_zone_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#instance_clauses}.
 * @param ctx the parse tree
 */
fn enter_instance_clauses(&mut self, _ctx: &Instance_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#instance_clauses}.
 * @param ctx the parse tree
 */
fn exit_instance_clauses(&mut self, _ctx: &Instance_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#security_clause}.
 * @param ctx the parse tree
 */
fn enter_security_clause(&mut self, _ctx: &Security_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#security_clause}.
 * @param ctx the parse tree
 */
fn exit_security_clause(&mut self, _ctx: &Security_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#domain}.
 * @param ctx the parse tree
 */
fn enter_domain(&mut self, _ctx: &DomainContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#domain}.
 * @param ctx the parse tree
 */
fn exit_domain(&mut self, _ctx: &DomainContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#database}.
 * @param ctx the parse tree
 */
fn enter_database(&mut self, _ctx: &DatabaseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#database}.
 * @param ctx the parse tree
 */
fn exit_database(&mut self, _ctx: &DatabaseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#edition_name}.
 * @param ctx the parse tree
 */
fn enter_edition_name(&mut self, _ctx: &Edition_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#edition_name}.
 * @param ctx the parse tree
 */
fn exit_edition_name(&mut self, _ctx: &Edition_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#filenumber}.
 * @param ctx the parse tree
 */
fn enter_filenumber(&mut self, _ctx: &FilenumberContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#filenumber}.
 * @param ctx the parse tree
 */
fn exit_filenumber(&mut self, _ctx: &FilenumberContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#filename}.
 * @param ctx the parse tree
 */
fn enter_filename(&mut self, _ctx: &FilenameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#filename}.
 * @param ctx the parse tree
 */
fn exit_filename(&mut self, _ctx: &FilenameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#prepare_clause}.
 * @param ctx the parse tree
 */
fn enter_prepare_clause(&mut self, _ctx: &Prepare_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#prepare_clause}.
 * @param ctx the parse tree
 */
fn exit_prepare_clause(&mut self, _ctx: &Prepare_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_mirror_clause}.
 * @param ctx the parse tree
 */
fn enter_drop_mirror_clause(&mut self, _ctx: &Drop_mirror_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_mirror_clause}.
 * @param ctx the parse tree
 */
fn exit_drop_mirror_clause(&mut self, _ctx: &Drop_mirror_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lost_write_protection}.
 * @param ctx the parse tree
 */
fn enter_lost_write_protection(&mut self, _ctx: &Lost_write_protectionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lost_write_protection}.
 * @param ctx the parse tree
 */
fn exit_lost_write_protection(&mut self, _ctx: &Lost_write_protectionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cdb_fleet_clauses}.
 * @param ctx the parse tree
 */
fn enter_cdb_fleet_clauses(&mut self, _ctx: &Cdb_fleet_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cdb_fleet_clauses}.
 * @param ctx the parse tree
 */
fn exit_cdb_fleet_clauses(&mut self, _ctx: &Cdb_fleet_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lead_cdb_clause}.
 * @param ctx the parse tree
 */
fn enter_lead_cdb_clause(&mut self, _ctx: &Lead_cdb_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lead_cdb_clause}.
 * @param ctx the parse tree
 */
fn exit_lead_cdb_clause(&mut self, _ctx: &Lead_cdb_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lead_cdb_uri_clause}.
 * @param ctx the parse tree
 */
fn enter_lead_cdb_uri_clause(&mut self, _ctx: &Lead_cdb_uri_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lead_cdb_uri_clause}.
 * @param ctx the parse tree
 */
fn exit_lead_cdb_uri_clause(&mut self, _ctx: &Lead_cdb_uri_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#property_clauses}.
 * @param ctx the parse tree
 */
fn enter_property_clauses(&mut self, _ctx: &Property_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#property_clauses}.
 * @param ctx the parse tree
 */
fn exit_property_clauses(&mut self, _ctx: &Property_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#replay_upgrade_clauses}.
 * @param ctx the parse tree
 */
fn enter_replay_upgrade_clauses(&mut self, _ctx: &Replay_upgrade_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#replay_upgrade_clauses}.
 * @param ctx the parse tree
 */
fn exit_replay_upgrade_clauses(&mut self, _ctx: &Replay_upgrade_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_database_link}.
 * @param ctx the parse tree
 */
fn enter_alter_database_link(&mut self, _ctx: &Alter_database_linkContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_database_link}.
 * @param ctx the parse tree
 */
fn exit_alter_database_link(&mut self, _ctx: &Alter_database_linkContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#password_value}.
 * @param ctx the parse tree
 */
fn enter_password_value(&mut self, _ctx: &Password_valueContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#password_value}.
 * @param ctx the parse tree
 */
fn exit_password_value(&mut self, _ctx: &Password_valueContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#link_authentication}.
 * @param ctx the parse tree
 */
fn enter_link_authentication(&mut self, _ctx: &Link_authenticationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#link_authentication}.
 * @param ctx the parse tree
 */
fn exit_link_authentication(&mut self, _ctx: &Link_authenticationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_schema}.
 * @param ctx the parse tree
 */
fn enter_create_schema(&mut self, _ctx: &Create_schemaContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_schema}.
 * @param ctx the parse tree
 */
fn exit_create_schema(&mut self, _ctx: &Create_schemaContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_database}.
 * @param ctx the parse tree
 */
fn enter_create_database(&mut self, _ctx: &Create_databaseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_database}.
 * @param ctx the parse tree
 */
fn exit_create_database(&mut self, _ctx: &Create_databaseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#database_logging_clauses}.
 * @param ctx the parse tree
 */
fn enter_database_logging_clauses(&mut self, _ctx: &Database_logging_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#database_logging_clauses}.
 * @param ctx the parse tree
 */
fn exit_database_logging_clauses(&mut self, _ctx: &Database_logging_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#database_logging_sub_clause}.
 * @param ctx the parse tree
 */
fn enter_database_logging_sub_clause(&mut self, _ctx: &Database_logging_sub_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#database_logging_sub_clause}.
 * @param ctx the parse tree
 */
fn exit_database_logging_sub_clause(&mut self, _ctx: &Database_logging_sub_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_clauses}.
 * @param ctx the parse tree
 */
fn enter_tablespace_clauses(&mut self, _ctx: &Tablespace_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_clauses}.
 * @param ctx the parse tree
 */
fn exit_tablespace_clauses(&mut self, _ctx: &Tablespace_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#enable_pluggable_database}.
 * @param ctx the parse tree
 */
fn enter_enable_pluggable_database(&mut self, _ctx: &Enable_pluggable_databaseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#enable_pluggable_database}.
 * @param ctx the parse tree
 */
fn exit_enable_pluggable_database(&mut self, _ctx: &Enable_pluggable_databaseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#file_name_convert}.
 * @param ctx the parse tree
 */
fn enter_file_name_convert(&mut self, _ctx: &File_name_convertContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#file_name_convert}.
 * @param ctx the parse tree
 */
fn exit_file_name_convert(&mut self, _ctx: &File_name_convertContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#filename_convert_sub_clause}.
 * @param ctx the parse tree
 */
fn enter_filename_convert_sub_clause(&mut self, _ctx: &Filename_convert_sub_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#filename_convert_sub_clause}.
 * @param ctx the parse tree
 */
fn exit_filename_convert_sub_clause(&mut self, _ctx: &Filename_convert_sub_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace_datafile_clauses}.
 * @param ctx the parse tree
 */
fn enter_tablespace_datafile_clauses(&mut self, _ctx: &Tablespace_datafile_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace_datafile_clauses}.
 * @param ctx the parse tree
 */
fn exit_tablespace_datafile_clauses(&mut self, _ctx: &Tablespace_datafile_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#undo_mode_clause}.
 * @param ctx the parse tree
 */
fn enter_undo_mode_clause(&mut self, _ctx: &Undo_mode_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#undo_mode_clause}.
 * @param ctx the parse tree
 */
fn exit_undo_mode_clause(&mut self, _ctx: &Undo_mode_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_tablespace}.
 * @param ctx the parse tree
 */
fn enter_default_tablespace(&mut self, _ctx: &Default_tablespaceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_tablespace}.
 * @param ctx the parse tree
 */
fn exit_default_tablespace(&mut self, _ctx: &Default_tablespaceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_temp_tablespace}.
 * @param ctx the parse tree
 */
fn enter_default_temp_tablespace(&mut self, _ctx: &Default_temp_tablespaceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_temp_tablespace}.
 * @param ctx the parse tree
 */
fn exit_default_temp_tablespace(&mut self, _ctx: &Default_temp_tablespaceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#undo_tablespace}.
 * @param ctx the parse tree
 */
fn enter_undo_tablespace(&mut self, _ctx: &Undo_tablespaceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#undo_tablespace}.
 * @param ctx the parse tree
 */
fn exit_undo_tablespace(&mut self, _ctx: &Undo_tablespaceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_database}.
 * @param ctx the parse tree
 */
fn enter_drop_database(&mut self, _ctx: &Drop_databaseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_database}.
 * @param ctx the parse tree
 */
fn exit_drop_database(&mut self, _ctx: &Drop_databaseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#create_database_link}.
 * @param ctx the parse tree
 */
fn enter_create_database_link(&mut self, _ctx: &Create_database_linkContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#create_database_link}.
 * @param ctx the parse tree
 */
fn exit_create_database_link(&mut self, _ctx: &Create_database_linkContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_database_link}.
 * @param ctx the parse tree
 */
fn enter_drop_database_link(&mut self, _ctx: &Drop_database_linkContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_database_link}.
 * @param ctx the parse tree
 */
fn exit_drop_database_link(&mut self, _ctx: &Drop_database_linkContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_tablespace_set}.
 * @param ctx the parse tree
 */
fn enter_alter_tablespace_set(&mut self, _ctx: &Alter_tablespace_setContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_tablespace_set}.
 * @param ctx the parse tree
 */
fn exit_alter_tablespace_set(&mut self, _ctx: &Alter_tablespace_setContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_tablespace_attrs}.
 * @param ctx the parse tree
 */
fn enter_alter_tablespace_attrs(&mut self, _ctx: &Alter_tablespace_attrsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_tablespace_attrs}.
 * @param ctx the parse tree
 */
fn exit_alter_tablespace_attrs(&mut self, _ctx: &Alter_tablespace_attrsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_tablespace_encryption}.
 * @param ctx the parse tree
 */
fn enter_alter_tablespace_encryption(&mut self, _ctx: &Alter_tablespace_encryptionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_tablespace_encryption}.
 * @param ctx the parse tree
 */
fn exit_alter_tablespace_encryption(&mut self, _ctx: &Alter_tablespace_encryptionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ts_file_name_convert}.
 * @param ctx the parse tree
 */
fn enter_ts_file_name_convert(&mut self, _ctx: &Ts_file_name_convertContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ts_file_name_convert}.
 * @param ctx the parse tree
 */
fn exit_ts_file_name_convert(&mut self, _ctx: &Ts_file_name_convertContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_role}.
 * @param ctx the parse tree
 */
fn enter_alter_role(&mut self, _ctx: &Alter_roleContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_role}.
 * @param ctx the parse tree
 */
fn exit_alter_role(&mut self, _ctx: &Alter_roleContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#role_identified_clause}.
 * @param ctx the parse tree
 */
fn enter_role_identified_clause(&mut self, _ctx: &Role_identified_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#role_identified_clause}.
 * @param ctx the parse tree
 */
fn exit_role_identified_clause(&mut self, _ctx: &Role_identified_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_table}.
 * @param ctx the parse tree
 */
fn enter_alter_table(&mut self, _ctx: &Alter_tableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_table}.
 * @param ctx the parse tree
 */
fn exit_alter_table(&mut self, _ctx: &Alter_tableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#memoptimize_read_write_clause}.
 * @param ctx the parse tree
 */
fn enter_memoptimize_read_write_clause(&mut self, _ctx: &Memoptimize_read_write_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#memoptimize_read_write_clause}.
 * @param ctx the parse tree
 */
fn exit_memoptimize_read_write_clause(&mut self, _ctx: &Memoptimize_read_write_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_table_properties}.
 * @param ctx the parse tree
 */
fn enter_alter_table_properties(&mut self, _ctx: &Alter_table_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_table_properties}.
 * @param ctx the parse tree
 */
fn exit_alter_table_properties(&mut self, _ctx: &Alter_table_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_table_partitioning}.
 * @param ctx the parse tree
 */
fn enter_alter_table_partitioning(&mut self, _ctx: &Alter_table_partitioningContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_table_partitioning}.
 * @param ctx the parse tree
 */
fn exit_alter_table_partitioning(&mut self, _ctx: &Alter_table_partitioningContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_table_partition}.
 * @param ctx the parse tree
 */
fn enter_add_table_partition(&mut self, _ctx: &Add_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_table_partition}.
 * @param ctx the parse tree
 */
fn exit_add_table_partition(&mut self, _ctx: &Add_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_table_partition}.
 * @param ctx the parse tree
 */
fn enter_drop_table_partition(&mut self, _ctx: &Drop_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_table_partition}.
 * @param ctx the parse tree
 */
fn exit_drop_table_partition(&mut self, _ctx: &Drop_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#merge_table_partition}.
 * @param ctx the parse tree
 */
fn enter_merge_table_partition(&mut self, _ctx: &Merge_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#merge_table_partition}.
 * @param ctx the parse tree
 */
fn exit_merge_table_partition(&mut self, _ctx: &Merge_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_table_partition}.
 * @param ctx the parse tree
 */
fn enter_modify_table_partition(&mut self, _ctx: &Modify_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_table_partition}.
 * @param ctx the parse tree
 */
fn exit_modify_table_partition(&mut self, _ctx: &Modify_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#split_table_partition}.
 * @param ctx the parse tree
 */
fn enter_split_table_partition(&mut self, _ctx: &Split_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#split_table_partition}.
 * @param ctx the parse tree
 */
fn exit_split_table_partition(&mut self, _ctx: &Split_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#truncate_table_partition}.
 * @param ctx the parse tree
 */
fn enter_truncate_table_partition(&mut self, _ctx: &Truncate_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#truncate_table_partition}.
 * @param ctx the parse tree
 */
fn exit_truncate_table_partition(&mut self, _ctx: &Truncate_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#exchange_table_partition}.
 * @param ctx the parse tree
 */
fn enter_exchange_table_partition(&mut self, _ctx: &Exchange_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#exchange_table_partition}.
 * @param ctx the parse tree
 */
fn exit_exchange_table_partition(&mut self, _ctx: &Exchange_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#coalesce_table_partition}.
 * @param ctx the parse tree
 */
fn enter_coalesce_table_partition(&mut self, _ctx: &Coalesce_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#coalesce_table_partition}.
 * @param ctx the parse tree
 */
fn exit_coalesce_table_partition(&mut self, _ctx: &Coalesce_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_interval_partition}.
 * @param ctx the parse tree
 */
fn enter_alter_interval_partition(&mut self, _ctx: &Alter_interval_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_interval_partition}.
 * @param ctx the parse tree
 */
fn exit_alter_interval_partition(&mut self, _ctx: &Alter_interval_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#move_table_partition}.
 * @param ctx the parse tree
 */
fn enter_move_table_partition(&mut self, _ctx: &Move_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#move_table_partition}.
 * @param ctx the parse tree
 */
fn exit_move_table_partition(&mut self, _ctx: &Move_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#filter_condition}.
 * @param ctx the parse tree
 */
fn enter_filter_condition(&mut self, _ctx: &Filter_conditionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#filter_condition}.
 * @param ctx the parse tree
 */
fn exit_filter_condition(&mut self, _ctx: &Filter_conditionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rename_table_partition}.
 * @param ctx the parse tree
 */
fn enter_rename_table_partition(&mut self, _ctx: &Rename_table_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rename_table_partition}.
 * @param ctx the parse tree
 */
fn exit_rename_table_partition(&mut self, _ctx: &Rename_table_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partition_extended_names}.
 * @param ctx the parse tree
 */
fn enter_partition_extended_names(&mut self, _ctx: &Partition_extended_namesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partition_extended_names}.
 * @param ctx the parse tree
 */
fn exit_partition_extended_names(&mut self, _ctx: &Partition_extended_namesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subpartition_extended_names}.
 * @param ctx the parse tree
 */
fn enter_subpartition_extended_names(&mut self, _ctx: &Subpartition_extended_namesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subpartition_extended_names}.
 * @param ctx the parse tree
 */
fn exit_subpartition_extended_names(&mut self, _ctx: &Subpartition_extended_namesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_table_properties_1}.
 * @param ctx the parse tree
 */
fn enter_alter_table_properties_1(&mut self, _ctx: &Alter_table_properties_1Context<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_table_properties_1}.
 * @param ctx the parse tree
 */
fn exit_alter_table_properties_1(&mut self, _ctx: &Alter_table_properties_1Context<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_iot_clauses}.
 * @param ctx the parse tree
 */
fn enter_alter_iot_clauses(&mut self, _ctx: &Alter_iot_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_iot_clauses}.
 * @param ctx the parse tree
 */
fn exit_alter_iot_clauses(&mut self, _ctx: &Alter_iot_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_mapping_table_clause}.
 * @param ctx the parse tree
 */
fn enter_alter_mapping_table_clause(&mut self, _ctx: &Alter_mapping_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_mapping_table_clause}.
 * @param ctx the parse tree
 */
fn exit_alter_mapping_table_clause(&mut self, _ctx: &Alter_mapping_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#alter_overflow_clause}.
 * @param ctx the parse tree
 */
fn enter_alter_overflow_clause(&mut self, _ctx: &Alter_overflow_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#alter_overflow_clause}.
 * @param ctx the parse tree
 */
fn exit_alter_overflow_clause(&mut self, _ctx: &Alter_overflow_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_overflow_clause}.
 * @param ctx the parse tree
 */
fn enter_add_overflow_clause(&mut self, _ctx: &Add_overflow_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_overflow_clause}.
 * @param ctx the parse tree
 */
fn exit_add_overflow_clause(&mut self, _ctx: &Add_overflow_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#update_index_clauses}.
 * @param ctx the parse tree
 */
fn enter_update_index_clauses(&mut self, _ctx: &Update_index_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#update_index_clauses}.
 * @param ctx the parse tree
 */
fn exit_update_index_clauses(&mut self, _ctx: &Update_index_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#update_global_index_clause}.
 * @param ctx the parse tree
 */
fn enter_update_global_index_clause(&mut self, _ctx: &Update_global_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#update_global_index_clause}.
 * @param ctx the parse tree
 */
fn exit_update_global_index_clause(&mut self, _ctx: &Update_global_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#update_all_indexes_clause}.
 * @param ctx the parse tree
 */
fn enter_update_all_indexes_clause(&mut self, _ctx: &Update_all_indexes_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#update_all_indexes_clause}.
 * @param ctx the parse tree
 */
fn exit_update_all_indexes_clause(&mut self, _ctx: &Update_all_indexes_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#update_all_indexes_index_clause}.
 * @param ctx the parse tree
 */
fn enter_update_all_indexes_index_clause(&mut self, _ctx: &Update_all_indexes_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#update_all_indexes_index_clause}.
 * @param ctx the parse tree
 */
fn exit_update_all_indexes_index_clause(&mut self, _ctx: &Update_all_indexes_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#update_index_partition}.
 * @param ctx the parse tree
 */
fn enter_update_index_partition(&mut self, _ctx: &Update_index_partitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#update_index_partition}.
 * @param ctx the parse tree
 */
fn exit_update_index_partition(&mut self, _ctx: &Update_index_partitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#update_index_subpartition}.
 * @param ctx the parse tree
 */
fn enter_update_index_subpartition(&mut self, _ctx: &Update_index_subpartitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#update_index_subpartition}.
 * @param ctx the parse tree
 */
fn exit_update_index_subpartition(&mut self, _ctx: &Update_index_subpartitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#enable_disable_clause}.
 * @param ctx the parse tree
 */
fn enter_enable_disable_clause(&mut self, _ctx: &Enable_disable_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#enable_disable_clause}.
 * @param ctx the parse tree
 */
fn exit_enable_disable_clause(&mut self, _ctx: &Enable_disable_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#using_index_clause}.
 * @param ctx the parse tree
 */
fn enter_using_index_clause(&mut self, _ctx: &Using_index_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#using_index_clause}.
 * @param ctx the parse tree
 */
fn exit_using_index_clause(&mut self, _ctx: &Using_index_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_attributes}.
 * @param ctx the parse tree
 */
fn enter_index_attributes(&mut self, _ctx: &Index_attributesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_attributes}.
 * @param ctx the parse tree
 */
fn exit_index_attributes(&mut self, _ctx: &Index_attributesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sort_or_nosort}.
 * @param ctx the parse tree
 */
fn enter_sort_or_nosort(&mut self, _ctx: &Sort_or_nosortContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sort_or_nosort}.
 * @param ctx the parse tree
 */
fn exit_sort_or_nosort(&mut self, _ctx: &Sort_or_nosortContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#exceptions_clause}.
 * @param ctx the parse tree
 */
fn enter_exceptions_clause(&mut self, _ctx: &Exceptions_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#exceptions_clause}.
 * @param ctx the parse tree
 */
fn exit_exceptions_clause(&mut self, _ctx: &Exceptions_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#move_table_clause}.
 * @param ctx the parse tree
 */
fn enter_move_table_clause(&mut self, _ctx: &Move_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#move_table_clause}.
 * @param ctx the parse tree
 */
fn exit_move_table_clause(&mut self, _ctx: &Move_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_org_table_clause}.
 * @param ctx the parse tree
 */
fn enter_index_org_table_clause(&mut self, _ctx: &Index_org_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_org_table_clause}.
 * @param ctx the parse tree
 */
fn exit_index_org_table_clause(&mut self, _ctx: &Index_org_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#mapping_table_clause}.
 * @param ctx the parse tree
 */
fn enter_mapping_table_clause(&mut self, _ctx: &Mapping_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#mapping_table_clause}.
 * @param ctx the parse tree
 */
fn exit_mapping_table_clause(&mut self, _ctx: &Mapping_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#key_compression}.
 * @param ctx the parse tree
 */
fn enter_key_compression(&mut self, _ctx: &Key_compressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#key_compression}.
 * @param ctx the parse tree
 */
fn exit_key_compression(&mut self, _ctx: &Key_compressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_org_overflow_clause}.
 * @param ctx the parse tree
 */
fn enter_index_org_overflow_clause(&mut self, _ctx: &Index_org_overflow_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_org_overflow_clause}.
 * @param ctx the parse tree
 */
fn exit_index_org_overflow_clause(&mut self, _ctx: &Index_org_overflow_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_clauses}.
 * @param ctx the parse tree
 */
fn enter_column_clauses(&mut self, _ctx: &Column_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_clauses}.
 * @param ctx the parse tree
 */
fn exit_column_clauses(&mut self, _ctx: &Column_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_collection_retrieval}.
 * @param ctx the parse tree
 */
fn enter_modify_collection_retrieval(&mut self, _ctx: &Modify_collection_retrievalContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_collection_retrieval}.
 * @param ctx the parse tree
 */
fn exit_modify_collection_retrieval(&mut self, _ctx: &Modify_collection_retrievalContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#collection_item}.
 * @param ctx the parse tree
 */
fn enter_collection_item(&mut self, _ctx: &Collection_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#collection_item}.
 * @param ctx the parse tree
 */
fn exit_collection_item(&mut self, _ctx: &Collection_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rename_column_clause}.
 * @param ctx the parse tree
 */
fn enter_rename_column_clause(&mut self, _ctx: &Rename_column_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rename_column_clause}.
 * @param ctx the parse tree
 */
fn exit_rename_column_clause(&mut self, _ctx: &Rename_column_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#old_column_name}.
 * @param ctx the parse tree
 */
fn enter_old_column_name(&mut self, _ctx: &Old_column_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#old_column_name}.
 * @param ctx the parse tree
 */
fn exit_old_column_name(&mut self, _ctx: &Old_column_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#new_column_name}.
 * @param ctx the parse tree
 */
fn enter_new_column_name(&mut self, _ctx: &New_column_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#new_column_name}.
 * @param ctx the parse tree
 */
fn exit_new_column_name(&mut self, _ctx: &New_column_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_modify_drop_column_clauses}.
 * @param ctx the parse tree
 */
fn enter_add_modify_drop_column_clauses(&mut self, _ctx: &Add_modify_drop_column_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_modify_drop_column_clauses}.
 * @param ctx the parse tree
 */
fn exit_add_modify_drop_column_clauses(&mut self, _ctx: &Add_modify_drop_column_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_column_clause}.
 * @param ctx the parse tree
 */
fn enter_drop_column_clause(&mut self, _ctx: &Drop_column_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_column_clause}.
 * @param ctx the parse tree
 */
fn exit_drop_column_clause(&mut self, _ctx: &Drop_column_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_column_clauses}.
 * @param ctx the parse tree
 */
fn enter_modify_column_clauses(&mut self, _ctx: &Modify_column_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_column_clauses}.
 * @param ctx the parse tree
 */
fn exit_modify_column_clauses(&mut self, _ctx: &Modify_column_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_col_properties}.
 * @param ctx the parse tree
 */
fn enter_modify_col_properties(&mut self, _ctx: &Modify_col_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_col_properties}.
 * @param ctx the parse tree
 */
fn exit_modify_col_properties(&mut self, _ctx: &Modify_col_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_col_visibility}.
 * @param ctx the parse tree
 */
fn enter_modify_col_visibility(&mut self, _ctx: &Modify_col_visibilityContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_col_visibility}.
 * @param ctx the parse tree
 */
fn exit_modify_col_visibility(&mut self, _ctx: &Modify_col_visibilityContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_col_substitutable}.
 * @param ctx the parse tree
 */
fn enter_modify_col_substitutable(&mut self, _ctx: &Modify_col_substitutableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_col_substitutable}.
 * @param ctx the parse tree
 */
fn exit_modify_col_substitutable(&mut self, _ctx: &Modify_col_substitutableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_column_clause}.
 * @param ctx the parse tree
 */
fn enter_add_column_clause(&mut self, _ctx: &Add_column_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_column_clause}.
 * @param ctx the parse tree
 */
fn exit_add_column_clause(&mut self, _ctx: &Add_column_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#varray_col_properties}.
 * @param ctx the parse tree
 */
fn enter_varray_col_properties(&mut self, _ctx: &Varray_col_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#varray_col_properties}.
 * @param ctx the parse tree
 */
fn exit_varray_col_properties(&mut self, _ctx: &Varray_col_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#varray_storage_clause}.
 * @param ctx the parse tree
 */
fn enter_varray_storage_clause(&mut self, _ctx: &Varray_storage_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#varray_storage_clause}.
 * @param ctx the parse tree
 */
fn exit_varray_storage_clause(&mut self, _ctx: &Varray_storage_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_segname}.
 * @param ctx the parse tree
 */
fn enter_lob_segname(&mut self, _ctx: &Lob_segnameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_segname}.
 * @param ctx the parse tree
 */
fn exit_lob_segname(&mut self, _ctx: &Lob_segnameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_item}.
 * @param ctx the parse tree
 */
fn enter_lob_item(&mut self, _ctx: &Lob_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_item}.
 * @param ctx the parse tree
 */
fn exit_lob_item(&mut self, _ctx: &Lob_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_storage_parameters}.
 * @param ctx the parse tree
 */
fn enter_lob_storage_parameters(&mut self, _ctx: &Lob_storage_parametersContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_storage_parameters}.
 * @param ctx the parse tree
 */
fn exit_lob_storage_parameters(&mut self, _ctx: &Lob_storage_parametersContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_storage_clause}.
 * @param ctx the parse tree
 */
fn enter_lob_storage_clause(&mut self, _ctx: &Lob_storage_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_storage_clause}.
 * @param ctx the parse tree
 */
fn exit_lob_storage_clause(&mut self, _ctx: &Lob_storage_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_lob_storage_clause}.
 * @param ctx the parse tree
 */
fn enter_modify_lob_storage_clause(&mut self, _ctx: &Modify_lob_storage_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_lob_storage_clause}.
 * @param ctx the parse tree
 */
fn exit_modify_lob_storage_clause(&mut self, _ctx: &Modify_lob_storage_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#modify_lob_parameters}.
 * @param ctx the parse tree
 */
fn enter_modify_lob_parameters(&mut self, _ctx: &Modify_lob_parametersContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#modify_lob_parameters}.
 * @param ctx the parse tree
 */
fn exit_modify_lob_parameters(&mut self, _ctx: &Modify_lob_parametersContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_parameters}.
 * @param ctx the parse tree
 */
fn enter_lob_parameters(&mut self, _ctx: &Lob_parametersContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_parameters}.
 * @param ctx the parse tree
 */
fn exit_lob_parameters(&mut self, _ctx: &Lob_parametersContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_deduplicate_clause}.
 * @param ctx the parse tree
 */
fn enter_lob_deduplicate_clause(&mut self, _ctx: &Lob_deduplicate_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_deduplicate_clause}.
 * @param ctx the parse tree
 */
fn exit_lob_deduplicate_clause(&mut self, _ctx: &Lob_deduplicate_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_compression_clause}.
 * @param ctx the parse tree
 */
fn enter_lob_compression_clause(&mut self, _ctx: &Lob_compression_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_compression_clause}.
 * @param ctx the parse tree
 */
fn exit_lob_compression_clause(&mut self, _ctx: &Lob_compression_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_retention_clause}.
 * @param ctx the parse tree
 */
fn enter_lob_retention_clause(&mut self, _ctx: &Lob_retention_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_retention_clause}.
 * @param ctx the parse tree
 */
fn exit_lob_retention_clause(&mut self, _ctx: &Lob_retention_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#encryption_spec}.
 * @param ctx the parse tree
 */
fn enter_encryption_spec(&mut self, _ctx: &Encryption_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#encryption_spec}.
 * @param ctx the parse tree
 */
fn exit_encryption_spec(&mut self, _ctx: &Encryption_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tablespace}.
 * @param ctx the parse tree
 */
fn enter_tablespace(&mut self, _ctx: &TablespaceContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tablespace}.
 * @param ctx the parse tree
 */
fn exit_tablespace(&mut self, _ctx: &TablespaceContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#varray_item}.
 * @param ctx the parse tree
 */
fn enter_varray_item(&mut self, _ctx: &Varray_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#varray_item}.
 * @param ctx the parse tree
 */
fn exit_varray_item(&mut self, _ctx: &Varray_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_properties}.
 * @param ctx the parse tree
 */
fn enter_column_properties(&mut self, _ctx: &Column_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_properties}.
 * @param ctx the parse tree
 */
fn exit_column_properties(&mut self, _ctx: &Column_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lob_partition_storage}.
 * @param ctx the parse tree
 */
fn enter_lob_partition_storage(&mut self, _ctx: &Lob_partition_storageContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lob_partition_storage}.
 * @param ctx the parse tree
 */
fn exit_lob_partition_storage(&mut self, _ctx: &Lob_partition_storageContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#period_definition}.
 * @param ctx the parse tree
 */
fn enter_period_definition(&mut self, _ctx: &Period_definitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#period_definition}.
 * @param ctx the parse tree
 */
fn exit_period_definition(&mut self, _ctx: &Period_definitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#start_time_column}.
 * @param ctx the parse tree
 */
fn enter_start_time_column(&mut self, _ctx: &Start_time_columnContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#start_time_column}.
 * @param ctx the parse tree
 */
fn exit_start_time_column(&mut self, _ctx: &Start_time_columnContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#end_time_column}.
 * @param ctx the parse tree
 */
fn enter_end_time_column(&mut self, _ctx: &End_time_columnContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#end_time_column}.
 * @param ctx the parse tree
 */
fn exit_end_time_column(&mut self, _ctx: &End_time_columnContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_definition}.
 * @param ctx the parse tree
 */
fn enter_column_definition(&mut self, _ctx: &Column_definitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_definition}.
 * @param ctx the parse tree
 */
fn exit_column_definition(&mut self, _ctx: &Column_definitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_collation_name}.
 * @param ctx the parse tree
 */
fn enter_column_collation_name(&mut self, _ctx: &Column_collation_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_collation_name}.
 * @param ctx the parse tree
 */
fn exit_column_collation_name(&mut self, _ctx: &Column_collation_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#identity_clause}.
 * @param ctx the parse tree
 */
fn enter_identity_clause(&mut self, _ctx: &Identity_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#identity_clause}.
 * @param ctx the parse tree
 */
fn exit_identity_clause(&mut self, _ctx: &Identity_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#identity_options_parentheses}.
 * @param ctx the parse tree
 */
fn enter_identity_options_parentheses(&mut self, _ctx: &Identity_options_parenthesesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#identity_options_parentheses}.
 * @param ctx the parse tree
 */
fn exit_identity_options_parentheses(&mut self, _ctx: &Identity_options_parenthesesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#identity_options}.
 * @param ctx the parse tree
 */
fn enter_identity_options(&mut self, _ctx: &Identity_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#identity_options}.
 * @param ctx the parse tree
 */
fn exit_identity_options(&mut self, _ctx: &Identity_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#virtual_column_definition}.
 * @param ctx the parse tree
 */
fn enter_virtual_column_definition(&mut self, _ctx: &Virtual_column_definitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#virtual_column_definition}.
 * @param ctx the parse tree
 */
fn exit_virtual_column_definition(&mut self, _ctx: &Virtual_column_definitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#virtual_column_expression}.
 * @param ctx the parse tree
 */
fn enter_virtual_column_expression(&mut self, _ctx: &Virtual_column_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#virtual_column_expression}.
 * @param ctx the parse tree
 */
fn exit_virtual_column_expression(&mut self, _ctx: &Virtual_column_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#autogenerated_sequence_definition}.
 * @param ctx the parse tree
 */
fn enter_autogenerated_sequence_definition(&mut self, _ctx: &Autogenerated_sequence_definitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#autogenerated_sequence_definition}.
 * @param ctx the parse tree
 */
fn exit_autogenerated_sequence_definition(&mut self, _ctx: &Autogenerated_sequence_definitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#by_user_for_statistics_clause}.
 * @param ctx the parse tree
 */
fn enter_by_user_for_statistics_clause(&mut self, _ctx: &By_user_for_statistics_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#by_user_for_statistics_clause}.
 * @param ctx the parse tree
 */
fn exit_by_user_for_statistics_clause(&mut self, _ctx: &By_user_for_statistics_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#evaluation_edition_clause}.
 * @param ctx the parse tree
 */
fn enter_evaluation_edition_clause(&mut self, _ctx: &Evaluation_edition_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#evaluation_edition_clause}.
 * @param ctx the parse tree
 */
fn exit_evaluation_edition_clause(&mut self, _ctx: &Evaluation_edition_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#nested_table_col_properties}.
 * @param ctx the parse tree
 */
fn enter_nested_table_col_properties(&mut self, _ctx: &Nested_table_col_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#nested_table_col_properties}.
 * @param ctx the parse tree
 */
fn exit_nested_table_col_properties(&mut self, _ctx: &Nested_table_col_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#nested_item}.
 * @param ctx the parse tree
 */
fn enter_nested_item(&mut self, _ctx: &Nested_itemContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#nested_item}.
 * @param ctx the parse tree
 */
fn exit_nested_item(&mut self, _ctx: &Nested_itemContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#substitutable_column_clause}.
 * @param ctx the parse tree
 */
fn enter_substitutable_column_clause(&mut self, _ctx: &Substitutable_column_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#substitutable_column_clause}.
 * @param ctx the parse tree
 */
fn exit_substitutable_column_clause(&mut self, _ctx: &Substitutable_column_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partition_name}.
 * @param ctx the parse tree
 */
fn enter_partition_name(&mut self, _ctx: &Partition_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partition_name}.
 * @param ctx the parse tree
 */
fn exit_partition_name(&mut self, _ctx: &Partition_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#supplemental_logging_props}.
 * @param ctx the parse tree
 */
fn enter_supplemental_logging_props(&mut self, _ctx: &Supplemental_logging_propsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#supplemental_logging_props}.
 * @param ctx the parse tree
 */
fn exit_supplemental_logging_props(&mut self, _ctx: &Supplemental_logging_propsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_type_col_properties}.
 * @param ctx the parse tree
 */
fn enter_object_type_col_properties(&mut self, _ctx: &Object_type_col_propertiesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_type_col_properties}.
 * @param ctx the parse tree
 */
fn exit_object_type_col_properties(&mut self, _ctx: &Object_type_col_propertiesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#constraint_clauses}.
 * @param ctx the parse tree
 */
fn enter_constraint_clauses(&mut self, _ctx: &Constraint_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#constraint_clauses}.
 * @param ctx the parse tree
 */
fn exit_constraint_clauses(&mut self, _ctx: &Constraint_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#old_constraint_name}.
 * @param ctx the parse tree
 */
fn enter_old_constraint_name(&mut self, _ctx: &Old_constraint_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#old_constraint_name}.
 * @param ctx the parse tree
 */
fn exit_old_constraint_name(&mut self, _ctx: &Old_constraint_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#new_constraint_name}.
 * @param ctx the parse tree
 */
fn enter_new_constraint_name(&mut self, _ctx: &New_constraint_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#new_constraint_name}.
 * @param ctx the parse tree
 */
fn exit_new_constraint_name(&mut self, _ctx: &New_constraint_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#drop_constraint_clause}.
 * @param ctx the parse tree
 */
fn enter_drop_constraint_clause(&mut self, _ctx: &Drop_constraint_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#drop_constraint_clause}.
 * @param ctx the parse tree
 */
fn exit_drop_constraint_clause(&mut self, _ctx: &Drop_constraint_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#check_constraint}.
 * @param ctx the parse tree
 */
fn enter_check_constraint(&mut self, _ctx: &Check_constraintContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#check_constraint}.
 * @param ctx the parse tree
 */
fn exit_check_constraint(&mut self, _ctx: &Check_constraintContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#foreign_key_clause}.
 * @param ctx the parse tree
 */
fn enter_foreign_key_clause(&mut self, _ctx: &Foreign_key_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#foreign_key_clause}.
 * @param ctx the parse tree
 */
fn exit_foreign_key_clause(&mut self, _ctx: &Foreign_key_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#references_clause}.
 * @param ctx the parse tree
 */
fn enter_references_clause(&mut self, _ctx: &References_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#references_clause}.
 * @param ctx the parse tree
 */
fn exit_references_clause(&mut self, _ctx: &References_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#on_delete_clause}.
 * @param ctx the parse tree
 */
fn enter_on_delete_clause(&mut self, _ctx: &On_delete_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#on_delete_clause}.
 * @param ctx the parse tree
 */
fn exit_on_delete_clause(&mut self, _ctx: &On_delete_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#anonymous_block}.
 * @param ctx the parse tree
 */
fn enter_anonymous_block(&mut self, _ctx: &Anonymous_blockContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#anonymous_block}.
 * @param ctx the parse tree
 */
fn exit_anonymous_block(&mut self, _ctx: &Anonymous_blockContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#invoker_rights_clause}.
 * @param ctx the parse tree
 */
fn enter_invoker_rights_clause(&mut self, _ctx: &Invoker_rights_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#invoker_rights_clause}.
 * @param ctx the parse tree
 */
fn exit_invoker_rights_clause(&mut self, _ctx: &Invoker_rights_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#call_spec}.
 * @param ctx the parse tree
 */
fn enter_call_spec(&mut self, _ctx: &Call_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#call_spec}.
 * @param ctx the parse tree
 */
fn exit_call_spec(&mut self, _ctx: &Call_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#java_spec}.
 * @param ctx the parse tree
 */
fn enter_java_spec(&mut self, _ctx: &Java_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#java_spec}.
 * @param ctx the parse tree
 */
fn exit_java_spec(&mut self, _ctx: &Java_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#c_spec}.
 * @param ctx the parse tree
 */
fn enter_c_spec(&mut self, _ctx: &C_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#c_spec}.
 * @param ctx the parse tree
 */
fn exit_c_spec(&mut self, _ctx: &C_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#c_agent_in_clause}.
 * @param ctx the parse tree
 */
fn enter_c_agent_in_clause(&mut self, _ctx: &C_agent_in_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#c_agent_in_clause}.
 * @param ctx the parse tree
 */
fn exit_c_agent_in_clause(&mut self, _ctx: &C_agent_in_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#c_parameters_clause}.
 * @param ctx the parse tree
 */
fn enter_c_parameters_clause(&mut self, _ctx: &C_parameters_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#c_parameters_clause}.
 * @param ctx the parse tree
 */
fn exit_c_parameters_clause(&mut self, _ctx: &C_parameters_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#c_external_parameter}.
 * @param ctx the parse tree
 */
fn enter_c_external_parameter(&mut self, _ctx: &C_external_parameterContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#c_external_parameter}.
 * @param ctx the parse tree
 */
fn exit_c_external_parameter(&mut self, _ctx: &C_external_parameterContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#c_property}.
 * @param ctx the parse tree
 */
fn enter_c_property(&mut self, _ctx: &C_propertyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#c_property}.
 * @param ctx the parse tree
 */
fn exit_c_property(&mut self, _ctx: &C_propertyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#parameter}.
 * @param ctx the parse tree
 */
fn enter_parameter(&mut self, _ctx: &ParameterContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#parameter}.
 * @param ctx the parse tree
 */
fn exit_parameter(&mut self, _ctx: &ParameterContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#default_value_part}.
 * @param ctx the parse tree
 */
fn enter_default_value_part(&mut self, _ctx: &Default_value_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#default_value_part}.
 * @param ctx the parse tree
 */
fn exit_default_value_part(&mut self, _ctx: &Default_value_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#seq_of_declare_specs}.
 * @param ctx the parse tree
 */
fn enter_seq_of_declare_specs(&mut self, _ctx: &Seq_of_declare_specsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#seq_of_declare_specs}.
 * @param ctx the parse tree
 */
fn exit_seq_of_declare_specs(&mut self, _ctx: &Seq_of_declare_specsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#declare_spec}.
 * @param ctx the parse tree
 */
fn enter_declare_spec(&mut self, _ctx: &Declare_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#declare_spec}.
 * @param ctx the parse tree
 */
fn exit_declare_spec(&mut self, _ctx: &Declare_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#variable_declaration}.
 * @param ctx the parse tree
 */
fn enter_variable_declaration(&mut self, _ctx: &Variable_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#variable_declaration}.
 * @param ctx the parse tree
 */
fn exit_variable_declaration(&mut self, _ctx: &Variable_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subtype_declaration}.
 * @param ctx the parse tree
 */
fn enter_subtype_declaration(&mut self, _ctx: &Subtype_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subtype_declaration}.
 * @param ctx the parse tree
 */
fn exit_subtype_declaration(&mut self, _ctx: &Subtype_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cursor_declaration}.
 * @param ctx the parse tree
 */
fn enter_cursor_declaration(&mut self, _ctx: &Cursor_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cursor_declaration}.
 * @param ctx the parse tree
 */
fn exit_cursor_declaration(&mut self, _ctx: &Cursor_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#parameter_spec}.
 * @param ctx the parse tree
 */
fn enter_parameter_spec(&mut self, _ctx: &Parameter_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#parameter_spec}.
 * @param ctx the parse tree
 */
fn exit_parameter_spec(&mut self, _ctx: &Parameter_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#exception_declaration}.
 * @param ctx the parse tree
 */
fn enter_exception_declaration(&mut self, _ctx: &Exception_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#exception_declaration}.
 * @param ctx the parse tree
 */
fn exit_exception_declaration(&mut self, _ctx: &Exception_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pragma_declaration}.
 * @param ctx the parse tree
 */
fn enter_pragma_declaration(&mut self, _ctx: &Pragma_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pragma_declaration}.
 * @param ctx the parse tree
 */
fn exit_pragma_declaration(&mut self, _ctx: &Pragma_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#record_type_def}.
 * @param ctx the parse tree
 */
fn enter_record_type_def(&mut self, _ctx: &Record_type_defContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#record_type_def}.
 * @param ctx the parse tree
 */
fn exit_record_type_def(&mut self, _ctx: &Record_type_defContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#field_spec}.
 * @param ctx the parse tree
 */
fn enter_field_spec(&mut self, _ctx: &Field_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#field_spec}.
 * @param ctx the parse tree
 */
fn exit_field_spec(&mut self, _ctx: &Field_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#ref_cursor_type_def}.
 * @param ctx the parse tree
 */
fn enter_ref_cursor_type_def(&mut self, _ctx: &Ref_cursor_type_defContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#ref_cursor_type_def}.
 * @param ctx the parse tree
 */
fn exit_ref_cursor_type_def(&mut self, _ctx: &Ref_cursor_type_defContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_declaration}.
 * @param ctx the parse tree
 */
fn enter_type_declaration(&mut self, _ctx: &Type_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_declaration}.
 * @param ctx the parse tree
 */
fn exit_type_declaration(&mut self, _ctx: &Type_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_type_def}.
 * @param ctx the parse tree
 */
fn enter_table_type_def(&mut self, _ctx: &Table_type_defContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_type_def}.
 * @param ctx the parse tree
 */
fn exit_table_type_def(&mut self, _ctx: &Table_type_defContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_indexed_by_part}.
 * @param ctx the parse tree
 */
fn enter_table_indexed_by_part(&mut self, _ctx: &Table_indexed_by_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_indexed_by_part}.
 * @param ctx the parse tree
 */
fn exit_table_indexed_by_part(&mut self, _ctx: &Table_indexed_by_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#varray_type_def}.
 * @param ctx the parse tree
 */
fn enter_varray_type_def(&mut self, _ctx: &Varray_type_defContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#varray_type_def}.
 * @param ctx the parse tree
 */
fn exit_varray_type_def(&mut self, _ctx: &Varray_type_defContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#seq_of_statements}.
 * @param ctx the parse tree
 */
fn enter_seq_of_statements(&mut self, _ctx: &Seq_of_statementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#seq_of_statements}.
 * @param ctx the parse tree
 */
fn exit_seq_of_statements(&mut self, _ctx: &Seq_of_statementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#label_declaration}.
 * @param ctx the parse tree
 */
fn enter_label_declaration(&mut self, _ctx: &Label_declarationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#label_declaration}.
 * @param ctx the parse tree
 */
fn exit_label_declaration(&mut self, _ctx: &Label_declarationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#statement}.
 * @param ctx the parse tree
 */
fn enter_statement(&mut self, _ctx: &StatementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#statement}.
 * @param ctx the parse tree
 */
fn exit_statement(&mut self, _ctx: &StatementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#assignment_statement}.
 * @param ctx the parse tree
 */
fn enter_assignment_statement(&mut self, _ctx: &Assignment_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#assignment_statement}.
 * @param ctx the parse tree
 */
fn exit_assignment_statement(&mut self, _ctx: &Assignment_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#continue_statement}.
 * @param ctx the parse tree
 */
fn enter_continue_statement(&mut self, _ctx: &Continue_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#continue_statement}.
 * @param ctx the parse tree
 */
fn exit_continue_statement(&mut self, _ctx: &Continue_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#exit_statement}.
 * @param ctx the parse tree
 */
fn enter_exit_statement(&mut self, _ctx: &Exit_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#exit_statement}.
 * @param ctx the parse tree
 */
fn exit_exit_statement(&mut self, _ctx: &Exit_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#goto_statement}.
 * @param ctx the parse tree
 */
fn enter_goto_statement(&mut self, _ctx: &Goto_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#goto_statement}.
 * @param ctx the parse tree
 */
fn exit_goto_statement(&mut self, _ctx: &Goto_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#if_statement}.
 * @param ctx the parse tree
 */
fn enter_if_statement(&mut self, _ctx: &If_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#if_statement}.
 * @param ctx the parse tree
 */
fn exit_if_statement(&mut self, _ctx: &If_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#elsif_part}.
 * @param ctx the parse tree
 */
fn enter_elsif_part(&mut self, _ctx: &Elsif_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#elsif_part}.
 * @param ctx the parse tree
 */
fn exit_elsif_part(&mut self, _ctx: &Elsif_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#else_part}.
 * @param ctx the parse tree
 */
fn enter_else_part(&mut self, _ctx: &Else_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#else_part}.
 * @param ctx the parse tree
 */
fn exit_else_part(&mut self, _ctx: &Else_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#loop_statement}.
 * @param ctx the parse tree
 */
fn enter_loop_statement(&mut self, _ctx: &Loop_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#loop_statement}.
 * @param ctx the parse tree
 */
fn exit_loop_statement(&mut self, _ctx: &Loop_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cursor_loop_param}.
 * @param ctx the parse tree
 */
fn enter_cursor_loop_param(&mut self, _ctx: &Cursor_loop_paramContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cursor_loop_param}.
 * @param ctx the parse tree
 */
fn exit_cursor_loop_param(&mut self, _ctx: &Cursor_loop_paramContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#forall_statement}.
 * @param ctx the parse tree
 */
fn enter_forall_statement(&mut self, _ctx: &Forall_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#forall_statement}.
 * @param ctx the parse tree
 */
fn exit_forall_statement(&mut self, _ctx: &Forall_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#bounds_clause}.
 * @param ctx the parse tree
 */
fn enter_bounds_clause(&mut self, _ctx: &Bounds_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#bounds_clause}.
 * @param ctx the parse tree
 */
fn exit_bounds_clause(&mut self, _ctx: &Bounds_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#between_bound}.
 * @param ctx the parse tree
 */
fn enter_between_bound(&mut self, _ctx: &Between_boundContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#between_bound}.
 * @param ctx the parse tree
 */
fn exit_between_bound(&mut self, _ctx: &Between_boundContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lower_bound}.
 * @param ctx the parse tree
 */
fn enter_lower_bound(&mut self, _ctx: &Lower_boundContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lower_bound}.
 * @param ctx the parse tree
 */
fn exit_lower_bound(&mut self, _ctx: &Lower_boundContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#upper_bound}.
 * @param ctx the parse tree
 */
fn enter_upper_bound(&mut self, _ctx: &Upper_boundContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#upper_bound}.
 * @param ctx the parse tree
 */
fn exit_upper_bound(&mut self, _ctx: &Upper_boundContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#null_statement}.
 * @param ctx the parse tree
 */
fn enter_null_statement(&mut self, _ctx: &Null_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#null_statement}.
 * @param ctx the parse tree
 */
fn exit_null_statement(&mut self, _ctx: &Null_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#raise_statement}.
 * @param ctx the parse tree
 */
fn enter_raise_statement(&mut self, _ctx: &Raise_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#raise_statement}.
 * @param ctx the parse tree
 */
fn exit_raise_statement(&mut self, _ctx: &Raise_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#return_statement}.
 * @param ctx the parse tree
 */
fn enter_return_statement(&mut self, _ctx: &Return_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#return_statement}.
 * @param ctx the parse tree
 */
fn exit_return_statement(&mut self, _ctx: &Return_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#call_statement}.
 * @param ctx the parse tree
 */
fn enter_call_statement(&mut self, _ctx: &Call_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#call_statement}.
 * @param ctx the parse tree
 */
fn exit_call_statement(&mut self, _ctx: &Call_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pipe_row_statement}.
 * @param ctx the parse tree
 */
fn enter_pipe_row_statement(&mut self, _ctx: &Pipe_row_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pipe_row_statement}.
 * @param ctx the parse tree
 */
fn exit_pipe_row_statement(&mut self, _ctx: &Pipe_row_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#selection_directive}.
 * @param ctx the parse tree
 */
fn enter_selection_directive(&mut self, _ctx: &Selection_directiveContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#selection_directive}.
 * @param ctx the parse tree
 */
fn exit_selection_directive(&mut self, _ctx: &Selection_directiveContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#error_directive}.
 * @param ctx the parse tree
 */
fn enter_error_directive(&mut self, _ctx: &Error_directiveContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#error_directive}.
 * @param ctx the parse tree
 */
fn exit_error_directive(&mut self, _ctx: &Error_directiveContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#selection_directive_body}.
 * @param ctx the parse tree
 */
fn enter_selection_directive_body(&mut self, _ctx: &Selection_directive_bodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#selection_directive_body}.
 * @param ctx the parse tree
 */
fn exit_selection_directive_body(&mut self, _ctx: &Selection_directive_bodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#body}.
 * @param ctx the parse tree
 */
fn enter_body(&mut self, _ctx: &BodyContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#body}.
 * @param ctx the parse tree
 */
fn exit_body(&mut self, _ctx: &BodyContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#exception_handler}.
 * @param ctx the parse tree
 */
fn enter_exception_handler(&mut self, _ctx: &Exception_handlerContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#exception_handler}.
 * @param ctx the parse tree
 */
fn exit_exception_handler(&mut self, _ctx: &Exception_handlerContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#trigger_block}.
 * @param ctx the parse tree
 */
fn enter_trigger_block(&mut self, _ctx: &Trigger_blockContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#trigger_block}.
 * @param ctx the parse tree
 */
fn exit_trigger_block(&mut self, _ctx: &Trigger_blockContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tps_block}.
 * @param ctx the parse tree
 */
fn enter_tps_block(&mut self, _ctx: &Tps_blockContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tps_block}.
 * @param ctx the parse tree
 */
fn exit_tps_block(&mut self, _ctx: &Tps_blockContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#block}.
 * @param ctx the parse tree
 */
fn enter_block(&mut self, _ctx: &BlockContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#block}.
 * @param ctx the parse tree
 */
fn exit_block(&mut self, _ctx: &BlockContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sql_statement}.
 * @param ctx the parse tree
 */
fn enter_sql_statement(&mut self, _ctx: &Sql_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sql_statement}.
 * @param ctx the parse tree
 */
fn exit_sql_statement(&mut self, _ctx: &Sql_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#execute_immediate}.
 * @param ctx the parse tree
 */
fn enter_execute_immediate(&mut self, _ctx: &Execute_immediateContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#execute_immediate}.
 * @param ctx the parse tree
 */
fn exit_execute_immediate(&mut self, _ctx: &Execute_immediateContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dynamic_returning_clause}.
 * @param ctx the parse tree
 */
fn enter_dynamic_returning_clause(&mut self, _ctx: &Dynamic_returning_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dynamic_returning_clause}.
 * @param ctx the parse tree
 */
fn exit_dynamic_returning_clause(&mut self, _ctx: &Dynamic_returning_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#data_manipulation_language_statements}.
 * @param ctx the parse tree
 */
fn enter_data_manipulation_language_statements(&mut self, _ctx: &Data_manipulation_language_statementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#data_manipulation_language_statements}.
 * @param ctx the parse tree
 */
fn exit_data_manipulation_language_statements(&mut self, _ctx: &Data_manipulation_language_statementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cursor_manipulation_statements}.
 * @param ctx the parse tree
 */
fn enter_cursor_manipulation_statements(&mut self, _ctx: &Cursor_manipulation_statementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cursor_manipulation_statements}.
 * @param ctx the parse tree
 */
fn exit_cursor_manipulation_statements(&mut self, _ctx: &Cursor_manipulation_statementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#close_statement}.
 * @param ctx the parse tree
 */
fn enter_close_statement(&mut self, _ctx: &Close_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#close_statement}.
 * @param ctx the parse tree
 */
fn exit_close_statement(&mut self, _ctx: &Close_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#open_statement}.
 * @param ctx the parse tree
 */
fn enter_open_statement(&mut self, _ctx: &Open_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#open_statement}.
 * @param ctx the parse tree
 */
fn exit_open_statement(&mut self, _ctx: &Open_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#fetch_statement}.
 * @param ctx the parse tree
 */
fn enter_fetch_statement(&mut self, _ctx: &Fetch_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#fetch_statement}.
 * @param ctx the parse tree
 */
fn exit_fetch_statement(&mut self, _ctx: &Fetch_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#variable_or_collection}.
 * @param ctx the parse tree
 */
fn enter_variable_or_collection(&mut self, _ctx: &Variable_or_collectionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#variable_or_collection}.
 * @param ctx the parse tree
 */
fn exit_variable_or_collection(&mut self, _ctx: &Variable_or_collectionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#open_for_statement}.
 * @param ctx the parse tree
 */
fn enter_open_for_statement(&mut self, _ctx: &Open_for_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#open_for_statement}.
 * @param ctx the parse tree
 */
fn exit_open_for_statement(&mut self, _ctx: &Open_for_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#transaction_control_statements}.
 * @param ctx the parse tree
 */
fn enter_transaction_control_statements(&mut self, _ctx: &Transaction_control_statementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#transaction_control_statements}.
 * @param ctx the parse tree
 */
fn exit_transaction_control_statements(&mut self, _ctx: &Transaction_control_statementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#set_transaction_command}.
 * @param ctx the parse tree
 */
fn enter_set_transaction_command(&mut self, _ctx: &Set_transaction_commandContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#set_transaction_command}.
 * @param ctx the parse tree
 */
fn exit_set_transaction_command(&mut self, _ctx: &Set_transaction_commandContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#set_constraint_command}.
 * @param ctx the parse tree
 */
fn enter_set_constraint_command(&mut self, _ctx: &Set_constraint_commandContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#set_constraint_command}.
 * @param ctx the parse tree
 */
fn exit_set_constraint_command(&mut self, _ctx: &Set_constraint_commandContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#commit_statement}.
 * @param ctx the parse tree
 */
fn enter_commit_statement(&mut self, _ctx: &Commit_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#commit_statement}.
 * @param ctx the parse tree
 */
fn exit_commit_statement(&mut self, _ctx: &Commit_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#write_clause}.
 * @param ctx the parse tree
 */
fn enter_write_clause(&mut self, _ctx: &Write_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#write_clause}.
 * @param ctx the parse tree
 */
fn exit_write_clause(&mut self, _ctx: &Write_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rollback_statement}.
 * @param ctx the parse tree
 */
fn enter_rollback_statement(&mut self, _ctx: &Rollback_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rollback_statement}.
 * @param ctx the parse tree
 */
fn exit_rollback_statement(&mut self, _ctx: &Rollback_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#savepoint_statement}.
 * @param ctx the parse tree
 */
fn enter_savepoint_statement(&mut self, _ctx: &Savepoint_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#savepoint_statement}.
 * @param ctx the parse tree
 */
fn exit_savepoint_statement(&mut self, _ctx: &Savepoint_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#collection_method_call}.
 * @param ctx the parse tree
 */
fn enter_collection_method_call(&mut self, _ctx: &Collection_method_callContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#collection_method_call}.
 * @param ctx the parse tree
 */
fn exit_collection_method_call(&mut self, _ctx: &Collection_method_callContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#explain_statement}.
 * @param ctx the parse tree
 */
fn enter_explain_statement(&mut self, _ctx: &Explain_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#explain_statement}.
 * @param ctx the parse tree
 */
fn exit_explain_statement(&mut self, _ctx: &Explain_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#select_only_statement}.
 * @param ctx the parse tree
 */
fn enter_select_only_statement(&mut self, _ctx: &Select_only_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#select_only_statement}.
 * @param ctx the parse tree
 */
fn exit_select_only_statement(&mut self, _ctx: &Select_only_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#select_statement}.
 * @param ctx the parse tree
 */
fn enter_select_statement(&mut self, _ctx: &Select_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#select_statement}.
 * @param ctx the parse tree
 */
fn exit_select_statement(&mut self, _ctx: &Select_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#with_clause}.
 * @param ctx the parse tree
 */
fn enter_with_clause(&mut self, _ctx: &With_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#with_clause}.
 * @param ctx the parse tree
 */
fn exit_with_clause(&mut self, _ctx: &With_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#with_factoring_clause}.
 * @param ctx the parse tree
 */
fn enter_with_factoring_clause(&mut self, _ctx: &With_factoring_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#with_factoring_clause}.
 * @param ctx the parse tree
 */
fn exit_with_factoring_clause(&mut self, _ctx: &With_factoring_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subquery_factoring_clause}.
 * @param ctx the parse tree
 */
fn enter_subquery_factoring_clause(&mut self, _ctx: &Subquery_factoring_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subquery_factoring_clause}.
 * @param ctx the parse tree
 */
fn exit_subquery_factoring_clause(&mut self, _ctx: &Subquery_factoring_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#search_clause}.
 * @param ctx the parse tree
 */
fn enter_search_clause(&mut self, _ctx: &Search_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#search_clause}.
 * @param ctx the parse tree
 */
fn exit_search_clause(&mut self, _ctx: &Search_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cycle_clause}.
 * @param ctx the parse tree
 */
fn enter_cycle_clause(&mut self, _ctx: &Cycle_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cycle_clause}.
 * @param ctx the parse tree
 */
fn exit_cycle_clause(&mut self, _ctx: &Cycle_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subav_factoring_clause}.
 * @param ctx the parse tree
 */
fn enter_subav_factoring_clause(&mut self, _ctx: &Subav_factoring_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subav_factoring_clause}.
 * @param ctx the parse tree
 */
fn exit_subav_factoring_clause(&mut self, _ctx: &Subav_factoring_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subav_clause}.
 * @param ctx the parse tree
 */
fn enter_subav_clause(&mut self, _ctx: &Subav_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subav_clause}.
 * @param ctx the parse tree
 */
fn exit_subav_clause(&mut self, _ctx: &Subav_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hierarchies_clause}.
 * @param ctx the parse tree
 */
fn enter_hierarchies_clause(&mut self, _ctx: &Hierarchies_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hierarchies_clause}.
 * @param ctx the parse tree
 */
fn exit_hierarchies_clause(&mut self, _ctx: &Hierarchies_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#filter_clauses}.
 * @param ctx the parse tree
 */
fn enter_filter_clauses(&mut self, _ctx: &Filter_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#filter_clauses}.
 * @param ctx the parse tree
 */
fn exit_filter_clauses(&mut self, _ctx: &Filter_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#filter_clause}.
 * @param ctx the parse tree
 */
fn enter_filter_clause(&mut self, _ctx: &Filter_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#filter_clause}.
 * @param ctx the parse tree
 */
fn exit_filter_clause(&mut self, _ctx: &Filter_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_calcs_clause}.
 * @param ctx the parse tree
 */
fn enter_add_calcs_clause(&mut self, _ctx: &Add_calcs_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_calcs_clause}.
 * @param ctx the parse tree
 */
fn exit_add_calcs_clause(&mut self, _ctx: &Add_calcs_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#add_calc_meas_clause}.
 * @param ctx the parse tree
 */
fn enter_add_calc_meas_clause(&mut self, _ctx: &Add_calc_meas_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#add_calc_meas_clause}.
 * @param ctx the parse tree
 */
fn exit_add_calc_meas_clause(&mut self, _ctx: &Add_calc_meas_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subquery}.
 * @param ctx the parse tree
 */
fn enter_subquery(&mut self, _ctx: &SubqueryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subquery}.
 * @param ctx the parse tree
 */
fn exit_subquery(&mut self, _ctx: &SubqueryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subquery_basic_elements}.
 * @param ctx the parse tree
 */
fn enter_subquery_basic_elements(&mut self, _ctx: &Subquery_basic_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subquery_basic_elements}.
 * @param ctx the parse tree
 */
fn exit_subquery_basic_elements(&mut self, _ctx: &Subquery_basic_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subquery_operation_part}.
 * @param ctx the parse tree
 */
fn enter_subquery_operation_part(&mut self, _ctx: &Subquery_operation_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subquery_operation_part}.
 * @param ctx the parse tree
 */
fn exit_subquery_operation_part(&mut self, _ctx: &Subquery_operation_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#query_block}.
 * @param ctx the parse tree
 */
fn enter_query_block(&mut self, _ctx: &Query_blockContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#query_block}.
 * @param ctx the parse tree
 */
fn exit_query_block(&mut self, _ctx: &Query_blockContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#selected_list}.
 * @param ctx the parse tree
 */
fn enter_selected_list(&mut self, _ctx: &Selected_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#selected_list}.
 * @param ctx the parse tree
 */
fn exit_selected_list(&mut self, _ctx: &Selected_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#from_clause}.
 * @param ctx the parse tree
 */
fn enter_from_clause(&mut self, _ctx: &From_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#from_clause}.
 * @param ctx the parse tree
 */
fn exit_from_clause(&mut self, _ctx: &From_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#select_list_elements}.
 * @param ctx the parse tree
 */
fn enter_select_list_elements(&mut self, _ctx: &Select_list_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#select_list_elements}.
 * @param ctx the parse tree
 */
fn exit_select_list_elements(&mut self, _ctx: &Select_list_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_ref_list}.
 * @param ctx the parse tree
 */
fn enter_table_ref_list(&mut self, _ctx: &Table_ref_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_ref_list}.
 * @param ctx the parse tree
 */
fn exit_table_ref_list(&mut self, _ctx: &Table_ref_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_ref}.
 * @param ctx the parse tree
 */
fn enter_table_ref(&mut self, _ctx: &Table_refContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_ref}.
 * @param ctx the parse tree
 */
fn exit_table_ref(&mut self, _ctx: &Table_refContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_ref_aux}.
 * @param ctx the parse tree
 */
fn enter_table_ref_aux(&mut self, _ctx: &Table_ref_auxContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_ref_aux}.
 * @param ctx the parse tree
 */
fn exit_table_ref_aux(&mut self, _ctx: &Table_ref_auxContext<'input>) { }
/**
 * Enter a parse tree produced by the {@code table_ref_aux_internal_one}
 * labeled alternative in {@link PlSqlParser#table_ref_aux_internal}.
 * @param ctx the parse tree
 */
fn enter_table_ref_aux_internal_one(&mut self, _ctx: &Table_ref_aux_internal_oneContext<'input>) { }
/**
 * Exit a parse tree produced by the {@code table_ref_aux_internal_one}
 * labeled alternative in {@link PlSqlParser#table_ref_aux_internal}.
 * @param ctx the parse tree
 */
fn exit_table_ref_aux_internal_one(&mut self, _ctx: &Table_ref_aux_internal_oneContext<'input>) { }
/**
 * Enter a parse tree produced by the {@code table_ref_aux_internal_two}
 * labeled alternative in {@link PlSqlParser#table_ref_aux_internal}.
 * @param ctx the parse tree
 */
fn enter_table_ref_aux_internal_two(&mut self, _ctx: &Table_ref_aux_internal_twoContext<'input>) { }
/**
 * Exit a parse tree produced by the {@code table_ref_aux_internal_two}
 * labeled alternative in {@link PlSqlParser#table_ref_aux_internal}.
 * @param ctx the parse tree
 */
fn exit_table_ref_aux_internal_two(&mut self, _ctx: &Table_ref_aux_internal_twoContext<'input>) { }
/**
 * Enter a parse tree produced by the {@code table_ref_aux_internal_thre}
 * labeled alternative in {@link PlSqlParser#table_ref_aux_internal}.
 * @param ctx the parse tree
 */
fn enter_table_ref_aux_internal_thre(&mut self, _ctx: &Table_ref_aux_internal_threContext<'input>) { }
/**
 * Exit a parse tree produced by the {@code table_ref_aux_internal_thre}
 * labeled alternative in {@link PlSqlParser#table_ref_aux_internal}.
 * @param ctx the parse tree
 */
fn exit_table_ref_aux_internal_thre(&mut self, _ctx: &Table_ref_aux_internal_threContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#join_clause}.
 * @param ctx the parse tree
 */
fn enter_join_clause(&mut self, _ctx: &Join_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#join_clause}.
 * @param ctx the parse tree
 */
fn exit_join_clause(&mut self, _ctx: &Join_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#join_on_part}.
 * @param ctx the parse tree
 */
fn enter_join_on_part(&mut self, _ctx: &Join_on_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#join_on_part}.
 * @param ctx the parse tree
 */
fn exit_join_on_part(&mut self, _ctx: &Join_on_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#join_using_part}.
 * @param ctx the parse tree
 */
fn enter_join_using_part(&mut self, _ctx: &Join_using_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#join_using_part}.
 * @param ctx the parse tree
 */
fn exit_join_using_part(&mut self, _ctx: &Join_using_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#outer_join_type}.
 * @param ctx the parse tree
 */
fn enter_outer_join_type(&mut self, _ctx: &Outer_join_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#outer_join_type}.
 * @param ctx the parse tree
 */
fn exit_outer_join_type(&mut self, _ctx: &Outer_join_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#query_partition_clause}.
 * @param ctx the parse tree
 */
fn enter_query_partition_clause(&mut self, _ctx: &Query_partition_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#query_partition_clause}.
 * @param ctx the parse tree
 */
fn exit_query_partition_clause(&mut self, _ctx: &Query_partition_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#flashback_query_clause}.
 * @param ctx the parse tree
 */
fn enter_flashback_query_clause(&mut self, _ctx: &Flashback_query_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#flashback_query_clause}.
 * @param ctx the parse tree
 */
fn exit_flashback_query_clause(&mut self, _ctx: &Flashback_query_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pivot_clause}.
 * @param ctx the parse tree
 */
fn enter_pivot_clause(&mut self, _ctx: &Pivot_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pivot_clause}.
 * @param ctx the parse tree
 */
fn exit_pivot_clause(&mut self, _ctx: &Pivot_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pivot_element}.
 * @param ctx the parse tree
 */
fn enter_pivot_element(&mut self, _ctx: &Pivot_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pivot_element}.
 * @param ctx the parse tree
 */
fn exit_pivot_element(&mut self, _ctx: &Pivot_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pivot_for_clause}.
 * @param ctx the parse tree
 */
fn enter_pivot_for_clause(&mut self, _ctx: &Pivot_for_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pivot_for_clause}.
 * @param ctx the parse tree
 */
fn exit_pivot_for_clause(&mut self, _ctx: &Pivot_for_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pivot_in_clause}.
 * @param ctx the parse tree
 */
fn enter_pivot_in_clause(&mut self, _ctx: &Pivot_in_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pivot_in_clause}.
 * @param ctx the parse tree
 */
fn exit_pivot_in_clause(&mut self, _ctx: &Pivot_in_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pivot_in_clause_element}.
 * @param ctx the parse tree
 */
fn enter_pivot_in_clause_element(&mut self, _ctx: &Pivot_in_clause_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pivot_in_clause_element}.
 * @param ctx the parse tree
 */
fn exit_pivot_in_clause_element(&mut self, _ctx: &Pivot_in_clause_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#pivot_in_clause_elements}.
 * @param ctx the parse tree
 */
fn enter_pivot_in_clause_elements(&mut self, _ctx: &Pivot_in_clause_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#pivot_in_clause_elements}.
 * @param ctx the parse tree
 */
fn exit_pivot_in_clause_elements(&mut self, _ctx: &Pivot_in_clause_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unpivot_clause}.
 * @param ctx the parse tree
 */
fn enter_unpivot_clause(&mut self, _ctx: &Unpivot_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unpivot_clause}.
 * @param ctx the parse tree
 */
fn exit_unpivot_clause(&mut self, _ctx: &Unpivot_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unpivot_in_clause}.
 * @param ctx the parse tree
 */
fn enter_unpivot_in_clause(&mut self, _ctx: &Unpivot_in_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unpivot_in_clause}.
 * @param ctx the parse tree
 */
fn exit_unpivot_in_clause(&mut self, _ctx: &Unpivot_in_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unpivot_in_elements}.
 * @param ctx the parse tree
 */
fn enter_unpivot_in_elements(&mut self, _ctx: &Unpivot_in_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unpivot_in_elements}.
 * @param ctx the parse tree
 */
fn exit_unpivot_in_elements(&mut self, _ctx: &Unpivot_in_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#hierarchical_query_clause}.
 * @param ctx the parse tree
 */
fn enter_hierarchical_query_clause(&mut self, _ctx: &Hierarchical_query_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#hierarchical_query_clause}.
 * @param ctx the parse tree
 */
fn exit_hierarchical_query_clause(&mut self, _ctx: &Hierarchical_query_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#start_part}.
 * @param ctx the parse tree
 */
fn enter_start_part(&mut self, _ctx: &Start_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#start_part}.
 * @param ctx the parse tree
 */
fn exit_start_part(&mut self, _ctx: &Start_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#group_by_clause}.
 * @param ctx the parse tree
 */
fn enter_group_by_clause(&mut self, _ctx: &Group_by_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#group_by_clause}.
 * @param ctx the parse tree
 */
fn exit_group_by_clause(&mut self, _ctx: &Group_by_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#group_by_elements}.
 * @param ctx the parse tree
 */
fn enter_group_by_elements(&mut self, _ctx: &Group_by_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#group_by_elements}.
 * @param ctx the parse tree
 */
fn exit_group_by_elements(&mut self, _ctx: &Group_by_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rollup_cube_clause}.
 * @param ctx the parse tree
 */
fn enter_rollup_cube_clause(&mut self, _ctx: &Rollup_cube_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rollup_cube_clause}.
 * @param ctx the parse tree
 */
fn exit_rollup_cube_clause(&mut self, _ctx: &Rollup_cube_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#grouping_sets_clause}.
 * @param ctx the parse tree
 */
fn enter_grouping_sets_clause(&mut self, _ctx: &Grouping_sets_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#grouping_sets_clause}.
 * @param ctx the parse tree
 */
fn exit_grouping_sets_clause(&mut self, _ctx: &Grouping_sets_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#grouping_sets_elements}.
 * @param ctx the parse tree
 */
fn enter_grouping_sets_elements(&mut self, _ctx: &Grouping_sets_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#grouping_sets_elements}.
 * @param ctx the parse tree
 */
fn exit_grouping_sets_elements(&mut self, _ctx: &Grouping_sets_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#having_clause}.
 * @param ctx the parse tree
 */
fn enter_having_clause(&mut self, _ctx: &Having_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#having_clause}.
 * @param ctx the parse tree
 */
fn exit_having_clause(&mut self, _ctx: &Having_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_clause}.
 * @param ctx the parse tree
 */
fn enter_model_clause(&mut self, _ctx: &Model_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_clause}.
 * @param ctx the parse tree
 */
fn exit_model_clause(&mut self, _ctx: &Model_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cell_reference_options}.
 * @param ctx the parse tree
 */
fn enter_cell_reference_options(&mut self, _ctx: &Cell_reference_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cell_reference_options}.
 * @param ctx the parse tree
 */
fn exit_cell_reference_options(&mut self, _ctx: &Cell_reference_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#return_rows_clause}.
 * @param ctx the parse tree
 */
fn enter_return_rows_clause(&mut self, _ctx: &Return_rows_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#return_rows_clause}.
 * @param ctx the parse tree
 */
fn exit_return_rows_clause(&mut self, _ctx: &Return_rows_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#reference_model}.
 * @param ctx the parse tree
 */
fn enter_reference_model(&mut self, _ctx: &Reference_modelContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#reference_model}.
 * @param ctx the parse tree
 */
fn exit_reference_model(&mut self, _ctx: &Reference_modelContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#main_model}.
 * @param ctx the parse tree
 */
fn enter_main_model(&mut self, _ctx: &Main_modelContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#main_model}.
 * @param ctx the parse tree
 */
fn exit_main_model(&mut self, _ctx: &Main_modelContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_column_clauses}.
 * @param ctx the parse tree
 */
fn enter_model_column_clauses(&mut self, _ctx: &Model_column_clausesContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_column_clauses}.
 * @param ctx the parse tree
 */
fn exit_model_column_clauses(&mut self, _ctx: &Model_column_clausesContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_column_partition_part}.
 * @param ctx the parse tree
 */
fn enter_model_column_partition_part(&mut self, _ctx: &Model_column_partition_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_column_partition_part}.
 * @param ctx the parse tree
 */
fn exit_model_column_partition_part(&mut self, _ctx: &Model_column_partition_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_column_list}.
 * @param ctx the parse tree
 */
fn enter_model_column_list(&mut self, _ctx: &Model_column_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_column_list}.
 * @param ctx the parse tree
 */
fn exit_model_column_list(&mut self, _ctx: &Model_column_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_column}.
 * @param ctx the parse tree
 */
fn enter_model_column(&mut self, _ctx: &Model_columnContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_column}.
 * @param ctx the parse tree
 */
fn exit_model_column(&mut self, _ctx: &Model_columnContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_rules_clause}.
 * @param ctx the parse tree
 */
fn enter_model_rules_clause(&mut self, _ctx: &Model_rules_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_rules_clause}.
 * @param ctx the parse tree
 */
fn exit_model_rules_clause(&mut self, _ctx: &Model_rules_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_rules_part}.
 * @param ctx the parse tree
 */
fn enter_model_rules_part(&mut self, _ctx: &Model_rules_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_rules_part}.
 * @param ctx the parse tree
 */
fn exit_model_rules_part(&mut self, _ctx: &Model_rules_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_rules_element}.
 * @param ctx the parse tree
 */
fn enter_model_rules_element(&mut self, _ctx: &Model_rules_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_rules_element}.
 * @param ctx the parse tree
 */
fn exit_model_rules_element(&mut self, _ctx: &Model_rules_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cell_assignment}.
 * @param ctx the parse tree
 */
fn enter_cell_assignment(&mut self, _ctx: &Cell_assignmentContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cell_assignment}.
 * @param ctx the parse tree
 */
fn exit_cell_assignment(&mut self, _ctx: &Cell_assignmentContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_iterate_clause}.
 * @param ctx the parse tree
 */
fn enter_model_iterate_clause(&mut self, _ctx: &Model_iterate_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_iterate_clause}.
 * @param ctx the parse tree
 */
fn exit_model_iterate_clause(&mut self, _ctx: &Model_iterate_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#until_part}.
 * @param ctx the parse tree
 */
fn enter_until_part(&mut self, _ctx: &Until_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#until_part}.
 * @param ctx the parse tree
 */
fn exit_until_part(&mut self, _ctx: &Until_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#order_by_clause}.
 * @param ctx the parse tree
 */
fn enter_order_by_clause(&mut self, _ctx: &Order_by_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#order_by_clause}.
 * @param ctx the parse tree
 */
fn exit_order_by_clause(&mut self, _ctx: &Order_by_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#order_by_elements}.
 * @param ctx the parse tree
 */
fn enter_order_by_elements(&mut self, _ctx: &Order_by_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#order_by_elements}.
 * @param ctx the parse tree
 */
fn exit_order_by_elements(&mut self, _ctx: &Order_by_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#offset_clause}.
 * @param ctx the parse tree
 */
fn enter_offset_clause(&mut self, _ctx: &Offset_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#offset_clause}.
 * @param ctx the parse tree
 */
fn exit_offset_clause(&mut self, _ctx: &Offset_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#fetch_clause}.
 * @param ctx the parse tree
 */
fn enter_fetch_clause(&mut self, _ctx: &Fetch_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#fetch_clause}.
 * @param ctx the parse tree
 */
fn exit_fetch_clause(&mut self, _ctx: &Fetch_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#for_update_clause}.
 * @param ctx the parse tree
 */
fn enter_for_update_clause(&mut self, _ctx: &For_update_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#for_update_clause}.
 * @param ctx the parse tree
 */
fn exit_for_update_clause(&mut self, _ctx: &For_update_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#for_update_of_part}.
 * @param ctx the parse tree
 */
fn enter_for_update_of_part(&mut self, _ctx: &For_update_of_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#for_update_of_part}.
 * @param ctx the parse tree
 */
fn exit_for_update_of_part(&mut self, _ctx: &For_update_of_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#for_update_options}.
 * @param ctx the parse tree
 */
fn enter_for_update_options(&mut self, _ctx: &For_update_optionsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#for_update_options}.
 * @param ctx the parse tree
 */
fn exit_for_update_options(&mut self, _ctx: &For_update_optionsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#update_statement}.
 * @param ctx the parse tree
 */
fn enter_update_statement(&mut self, _ctx: &Update_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#update_statement}.
 * @param ctx the parse tree
 */
fn exit_update_statement(&mut self, _ctx: &Update_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#update_set_clause}.
 * @param ctx the parse tree
 */
fn enter_update_set_clause(&mut self, _ctx: &Update_set_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#update_set_clause}.
 * @param ctx the parse tree
 */
fn exit_update_set_clause(&mut self, _ctx: &Update_set_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_based_update_set_clause}.
 * @param ctx the parse tree
 */
fn enter_column_based_update_set_clause(&mut self, _ctx: &Column_based_update_set_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_based_update_set_clause}.
 * @param ctx the parse tree
 */
fn exit_column_based_update_set_clause(&mut self, _ctx: &Column_based_update_set_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#delete_statement}.
 * @param ctx the parse tree
 */
fn enter_delete_statement(&mut self, _ctx: &Delete_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#delete_statement}.
 * @param ctx the parse tree
 */
fn exit_delete_statement(&mut self, _ctx: &Delete_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#insert_statement}.
 * @param ctx the parse tree
 */
fn enter_insert_statement(&mut self, _ctx: &Insert_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#insert_statement}.
 * @param ctx the parse tree
 */
fn exit_insert_statement(&mut self, _ctx: &Insert_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#single_table_insert}.
 * @param ctx the parse tree
 */
fn enter_single_table_insert(&mut self, _ctx: &Single_table_insertContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#single_table_insert}.
 * @param ctx the parse tree
 */
fn exit_single_table_insert(&mut self, _ctx: &Single_table_insertContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#multi_table_insert}.
 * @param ctx the parse tree
 */
fn enter_multi_table_insert(&mut self, _ctx: &Multi_table_insertContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#multi_table_insert}.
 * @param ctx the parse tree
 */
fn exit_multi_table_insert(&mut self, _ctx: &Multi_table_insertContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#multi_table_element}.
 * @param ctx the parse tree
 */
fn enter_multi_table_element(&mut self, _ctx: &Multi_table_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#multi_table_element}.
 * @param ctx the parse tree
 */
fn exit_multi_table_element(&mut self, _ctx: &Multi_table_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#conditional_insert_clause}.
 * @param ctx the parse tree
 */
fn enter_conditional_insert_clause(&mut self, _ctx: &Conditional_insert_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#conditional_insert_clause}.
 * @param ctx the parse tree
 */
fn exit_conditional_insert_clause(&mut self, _ctx: &Conditional_insert_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#conditional_insert_when_part}.
 * @param ctx the parse tree
 */
fn enter_conditional_insert_when_part(&mut self, _ctx: &Conditional_insert_when_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#conditional_insert_when_part}.
 * @param ctx the parse tree
 */
fn exit_conditional_insert_when_part(&mut self, _ctx: &Conditional_insert_when_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#conditional_insert_else_part}.
 * @param ctx the parse tree
 */
fn enter_conditional_insert_else_part(&mut self, _ctx: &Conditional_insert_else_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#conditional_insert_else_part}.
 * @param ctx the parse tree
 */
fn exit_conditional_insert_else_part(&mut self, _ctx: &Conditional_insert_else_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#insert_into_clause}.
 * @param ctx the parse tree
 */
fn enter_insert_into_clause(&mut self, _ctx: &Insert_into_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#insert_into_clause}.
 * @param ctx the parse tree
 */
fn exit_insert_into_clause(&mut self, _ctx: &Insert_into_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#values_clause}.
 * @param ctx the parse tree
 */
fn enter_values_clause(&mut self, _ctx: &Values_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#values_clause}.
 * @param ctx the parse tree
 */
fn exit_values_clause(&mut self, _ctx: &Values_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#merge_statement}.
 * @param ctx the parse tree
 */
fn enter_merge_statement(&mut self, _ctx: &Merge_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#merge_statement}.
 * @param ctx the parse tree
 */
fn exit_merge_statement(&mut self, _ctx: &Merge_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#merge_update_clause}.
 * @param ctx the parse tree
 */
fn enter_merge_update_clause(&mut self, _ctx: &Merge_update_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#merge_update_clause}.
 * @param ctx the parse tree
 */
fn exit_merge_update_clause(&mut self, _ctx: &Merge_update_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#merge_element}.
 * @param ctx the parse tree
 */
fn enter_merge_element(&mut self, _ctx: &Merge_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#merge_element}.
 * @param ctx the parse tree
 */
fn exit_merge_element(&mut self, _ctx: &Merge_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#merge_update_delete_part}.
 * @param ctx the parse tree
 */
fn enter_merge_update_delete_part(&mut self, _ctx: &Merge_update_delete_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#merge_update_delete_part}.
 * @param ctx the parse tree
 */
fn exit_merge_update_delete_part(&mut self, _ctx: &Merge_update_delete_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#merge_insert_clause}.
 * @param ctx the parse tree
 */
fn enter_merge_insert_clause(&mut self, _ctx: &Merge_insert_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#merge_insert_clause}.
 * @param ctx the parse tree
 */
fn exit_merge_insert_clause(&mut self, _ctx: &Merge_insert_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#selected_tableview}.
 * @param ctx the parse tree
 */
fn enter_selected_tableview(&mut self, _ctx: &Selected_tableviewContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#selected_tableview}.
 * @param ctx the parse tree
 */
fn exit_selected_tableview(&mut self, _ctx: &Selected_tableviewContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lock_table_statement}.
 * @param ctx the parse tree
 */
fn enter_lock_table_statement(&mut self, _ctx: &Lock_table_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lock_table_statement}.
 * @param ctx the parse tree
 */
fn exit_lock_table_statement(&mut self, _ctx: &Lock_table_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#wait_nowait_part}.
 * @param ctx the parse tree
 */
fn enter_wait_nowait_part(&mut self, _ctx: &Wait_nowait_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#wait_nowait_part}.
 * @param ctx the parse tree
 */
fn exit_wait_nowait_part(&mut self, _ctx: &Wait_nowait_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lock_table_element}.
 * @param ctx the parse tree
 */
fn enter_lock_table_element(&mut self, _ctx: &Lock_table_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lock_table_element}.
 * @param ctx the parse tree
 */
fn exit_lock_table_element(&mut self, _ctx: &Lock_table_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#lock_mode}.
 * @param ctx the parse tree
 */
fn enter_lock_mode(&mut self, _ctx: &Lock_modeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#lock_mode}.
 * @param ctx the parse tree
 */
fn exit_lock_mode(&mut self, _ctx: &Lock_modeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#general_table_ref}.
 * @param ctx the parse tree
 */
fn enter_general_table_ref(&mut self, _ctx: &General_table_refContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#general_table_ref}.
 * @param ctx the parse tree
 */
fn exit_general_table_ref(&mut self, _ctx: &General_table_refContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#static_returning_clause}.
 * @param ctx the parse tree
 */
fn enter_static_returning_clause(&mut self, _ctx: &Static_returning_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#static_returning_clause}.
 * @param ctx the parse tree
 */
fn exit_static_returning_clause(&mut self, _ctx: &Static_returning_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#error_logging_clause}.
 * @param ctx the parse tree
 */
fn enter_error_logging_clause(&mut self, _ctx: &Error_logging_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#error_logging_clause}.
 * @param ctx the parse tree
 */
fn exit_error_logging_clause(&mut self, _ctx: &Error_logging_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#error_logging_into_part}.
 * @param ctx the parse tree
 */
fn enter_error_logging_into_part(&mut self, _ctx: &Error_logging_into_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#error_logging_into_part}.
 * @param ctx the parse tree
 */
fn exit_error_logging_into_part(&mut self, _ctx: &Error_logging_into_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#error_logging_reject_part}.
 * @param ctx the parse tree
 */
fn enter_error_logging_reject_part(&mut self, _ctx: &Error_logging_reject_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#error_logging_reject_part}.
 * @param ctx the parse tree
 */
fn exit_error_logging_reject_part(&mut self, _ctx: &Error_logging_reject_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dml_table_expression_clause}.
 * @param ctx the parse tree
 */
fn enter_dml_table_expression_clause(&mut self, _ctx: &Dml_table_expression_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dml_table_expression_clause}.
 * @param ctx the parse tree
 */
fn exit_dml_table_expression_clause(&mut self, _ctx: &Dml_table_expression_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_collection_expression}.
 * @param ctx the parse tree
 */
fn enter_table_collection_expression(&mut self, _ctx: &Table_collection_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_collection_expression}.
 * @param ctx the parse tree
 */
fn exit_table_collection_expression(&mut self, _ctx: &Table_collection_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#subquery_restriction_clause}.
 * @param ctx the parse tree
 */
fn enter_subquery_restriction_clause(&mut self, _ctx: &Subquery_restriction_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#subquery_restriction_clause}.
 * @param ctx the parse tree
 */
fn exit_subquery_restriction_clause(&mut self, _ctx: &Subquery_restriction_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sample_clause}.
 * @param ctx the parse tree
 */
fn enter_sample_clause(&mut self, _ctx: &Sample_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sample_clause}.
 * @param ctx the parse tree
 */
fn exit_sample_clause(&mut self, _ctx: &Sample_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#seed_part}.
 * @param ctx the parse tree
 */
fn enter_seed_part(&mut self, _ctx: &Seed_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#seed_part}.
 * @param ctx the parse tree
 */
fn exit_seed_part(&mut self, _ctx: &Seed_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#condition}.
 * @param ctx the parse tree
 */
fn enter_condition(&mut self, _ctx: &ConditionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#condition}.
 * @param ctx the parse tree
 */
fn exit_condition(&mut self, _ctx: &ConditionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#expressions_}.
 * @param ctx the parse tree
 */
fn enter_expressions_(&mut self, _ctx: &Expressions_Context<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#expressions_}.
 * @param ctx the parse tree
 */
fn exit_expressions_(&mut self, _ctx: &Expressions_Context<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#expression}.
 * @param ctx the parse tree
 */
fn enter_expression(&mut self, _ctx: &ExpressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#expression}.
 * @param ctx the parse tree
 */
fn exit_expression(&mut self, _ctx: &ExpressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cursor_expression}.
 * @param ctx the parse tree
 */
fn enter_cursor_expression(&mut self, _ctx: &Cursor_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cursor_expression}.
 * @param ctx the parse tree
 */
fn exit_cursor_expression(&mut self, _ctx: &Cursor_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#logical_expression}.
 * @param ctx the parse tree
 */
fn enter_logical_expression(&mut self, _ctx: &Logical_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#logical_expression}.
 * @param ctx the parse tree
 */
fn exit_logical_expression(&mut self, _ctx: &Logical_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unary_logical_expression}.
 * @param ctx the parse tree
 */
fn enter_unary_logical_expression(&mut self, _ctx: &Unary_logical_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unary_logical_expression}.
 * @param ctx the parse tree
 */
fn exit_unary_logical_expression(&mut self, _ctx: &Unary_logical_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unary_logical_operation}.
 * @param ctx the parse tree
 */
fn enter_unary_logical_operation(&mut self, _ctx: &Unary_logical_operationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unary_logical_operation}.
 * @param ctx the parse tree
 */
fn exit_unary_logical_operation(&mut self, _ctx: &Unary_logical_operationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#logical_operation}.
 * @param ctx the parse tree
 */
fn enter_logical_operation(&mut self, _ctx: &Logical_operationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#logical_operation}.
 * @param ctx the parse tree
 */
fn exit_logical_operation(&mut self, _ctx: &Logical_operationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#multiset_expression}.
 * @param ctx the parse tree
 */
fn enter_multiset_expression(&mut self, _ctx: &Multiset_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#multiset_expression}.
 * @param ctx the parse tree
 */
fn exit_multiset_expression(&mut self, _ctx: &Multiset_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#relational_expression}.
 * @param ctx the parse tree
 */
fn enter_relational_expression(&mut self, _ctx: &Relational_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#relational_expression}.
 * @param ctx the parse tree
 */
fn exit_relational_expression(&mut self, _ctx: &Relational_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#compound_expression}.
 * @param ctx the parse tree
 */
fn enter_compound_expression(&mut self, _ctx: &Compound_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#compound_expression}.
 * @param ctx the parse tree
 */
fn exit_compound_expression(&mut self, _ctx: &Compound_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#relational_operator}.
 * @param ctx the parse tree
 */
fn enter_relational_operator(&mut self, _ctx: &Relational_operatorContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#relational_operator}.
 * @param ctx the parse tree
 */
fn exit_relational_operator(&mut self, _ctx: &Relational_operatorContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#in_elements}.
 * @param ctx the parse tree
 */
fn enter_in_elements(&mut self, _ctx: &In_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#in_elements}.
 * @param ctx the parse tree
 */
fn exit_in_elements(&mut self, _ctx: &In_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#between_elements}.
 * @param ctx the parse tree
 */
fn enter_between_elements(&mut self, _ctx: &Between_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#between_elements}.
 * @param ctx the parse tree
 */
fn exit_between_elements(&mut self, _ctx: &Between_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#concatenation}.
 * @param ctx the parse tree
 */
fn enter_concatenation(&mut self, _ctx: &ConcatenationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#concatenation}.
 * @param ctx the parse tree
 */
fn exit_concatenation(&mut self, _ctx: &ConcatenationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#interval_expression}.
 * @param ctx the parse tree
 */
fn enter_interval_expression(&mut self, _ctx: &Interval_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#interval_expression}.
 * @param ctx the parse tree
 */
fn exit_interval_expression(&mut self, _ctx: &Interval_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_expression}.
 * @param ctx the parse tree
 */
fn enter_model_expression(&mut self, _ctx: &Model_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_expression}.
 * @param ctx the parse tree
 */
fn exit_model_expression(&mut self, _ctx: &Model_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#model_expression_element}.
 * @param ctx the parse tree
 */
fn enter_model_expression_element(&mut self, _ctx: &Model_expression_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#model_expression_element}.
 * @param ctx the parse tree
 */
fn exit_model_expression_element(&mut self, _ctx: &Model_expression_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#single_column_for_loop}.
 * @param ctx the parse tree
 */
fn enter_single_column_for_loop(&mut self, _ctx: &Single_column_for_loopContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#single_column_for_loop}.
 * @param ctx the parse tree
 */
fn exit_single_column_for_loop(&mut self, _ctx: &Single_column_for_loopContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#multi_column_for_loop}.
 * @param ctx the parse tree
 */
fn enter_multi_column_for_loop(&mut self, _ctx: &Multi_column_for_loopContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#multi_column_for_loop}.
 * @param ctx the parse tree
 */
fn exit_multi_column_for_loop(&mut self, _ctx: &Multi_column_for_loopContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#unary_expression}.
 * @param ctx the parse tree
 */
fn enter_unary_expression(&mut self, _ctx: &Unary_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#unary_expression}.
 * @param ctx the parse tree
 */
fn exit_unary_expression(&mut self, _ctx: &Unary_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#implicit_cursor_expression}.
 * @param ctx the parse tree
 */
fn enter_implicit_cursor_expression(&mut self, _ctx: &Implicit_cursor_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#implicit_cursor_expression}.
 * @param ctx the parse tree
 */
fn exit_implicit_cursor_expression(&mut self, _ctx: &Implicit_cursor_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#collection_expression}.
 * @param ctx the parse tree
 */
fn enter_collection_expression(&mut self, _ctx: &Collection_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#collection_expression}.
 * @param ctx the parse tree
 */
fn exit_collection_expression(&mut self, _ctx: &Collection_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#case_statement}.
 * @param ctx the parse tree
 */
fn enter_case_statement(&mut self, _ctx: &Case_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#case_statement}.
 * @param ctx the parse tree
 */
fn exit_case_statement(&mut self, _ctx: &Case_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#simple_case_statement}.
 * @param ctx the parse tree
 */
fn enter_simple_case_statement(&mut self, _ctx: &Simple_case_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#simple_case_statement}.
 * @param ctx the parse tree
 */
fn exit_simple_case_statement(&mut self, _ctx: &Simple_case_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#searched_case_statement}.
 * @param ctx the parse tree
 */
fn enter_searched_case_statement(&mut self, _ctx: &Searched_case_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#searched_case_statement}.
 * @param ctx the parse tree
 */
fn exit_searched_case_statement(&mut self, _ctx: &Searched_case_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#case_when_part_statement}.
 * @param ctx the parse tree
 */
fn enter_case_when_part_statement(&mut self, _ctx: &Case_when_part_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#case_when_part_statement}.
 * @param ctx the parse tree
 */
fn exit_case_when_part_statement(&mut self, _ctx: &Case_when_part_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#case_else_part_statement}.
 * @param ctx the parse tree
 */
fn enter_case_else_part_statement(&mut self, _ctx: &Case_else_part_statementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#case_else_part_statement}.
 * @param ctx the parse tree
 */
fn exit_case_else_part_statement(&mut self, _ctx: &Case_else_part_statementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#case_expression}.
 * @param ctx the parse tree
 */
fn enter_case_expression(&mut self, _ctx: &Case_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#case_expression}.
 * @param ctx the parse tree
 */
fn exit_case_expression(&mut self, _ctx: &Case_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#simple_case_expression}.
 * @param ctx the parse tree
 */
fn enter_simple_case_expression(&mut self, _ctx: &Simple_case_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#simple_case_expression}.
 * @param ctx the parse tree
 */
fn exit_simple_case_expression(&mut self, _ctx: &Simple_case_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#searched_case_expression}.
 * @param ctx the parse tree
 */
fn enter_searched_case_expression(&mut self, _ctx: &Searched_case_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#searched_case_expression}.
 * @param ctx the parse tree
 */
fn exit_searched_case_expression(&mut self, _ctx: &Searched_case_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#case_when_part_expression}.
 * @param ctx the parse tree
 */
fn enter_case_when_part_expression(&mut self, _ctx: &Case_when_part_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#case_when_part_expression}.
 * @param ctx the parse tree
 */
fn exit_case_when_part_expression(&mut self, _ctx: &Case_when_part_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#case_else_part_expression}.
 * @param ctx the parse tree
 */
fn enter_case_else_part_expression(&mut self, _ctx: &Case_else_part_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#case_else_part_expression}.
 * @param ctx the parse tree
 */
fn exit_case_else_part_expression(&mut self, _ctx: &Case_else_part_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#atom}.
 * @param ctx the parse tree
 */
fn enter_atom(&mut self, _ctx: &AtomContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#atom}.
 * @param ctx the parse tree
 */
fn exit_atom(&mut self, _ctx: &AtomContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#quantified_expression}.
 * @param ctx the parse tree
 */
fn enter_quantified_expression(&mut self, _ctx: &Quantified_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#quantified_expression}.
 * @param ctx the parse tree
 */
fn exit_quantified_expression(&mut self, _ctx: &Quantified_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#string_function}.
 * @param ctx the parse tree
 */
fn enter_string_function(&mut self, _ctx: &String_functionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#string_function}.
 * @param ctx the parse tree
 */
fn exit_string_function(&mut self, _ctx: &String_functionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#standard_function}.
 * @param ctx the parse tree
 */
fn enter_standard_function(&mut self, _ctx: &Standard_functionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#standard_function}.
 * @param ctx the parse tree
 */
fn exit_standard_function(&mut self, _ctx: &Standard_functionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_function}.
 * @param ctx the parse tree
 */
fn enter_json_function(&mut self, _ctx: &Json_functionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_function}.
 * @param ctx the parse tree
 */
fn exit_json_function(&mut self, _ctx: &Json_functionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_object_content}.
 * @param ctx the parse tree
 */
fn enter_json_object_content(&mut self, _ctx: &Json_object_contentContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_object_content}.
 * @param ctx the parse tree
 */
fn exit_json_object_content(&mut self, _ctx: &Json_object_contentContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_object_entry}.
 * @param ctx the parse tree
 */
fn enter_json_object_entry(&mut self, _ctx: &Json_object_entryContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_object_entry}.
 * @param ctx the parse tree
 */
fn exit_json_object_entry(&mut self, _ctx: &Json_object_entryContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_table_clause}.
 * @param ctx the parse tree
 */
fn enter_json_table_clause(&mut self, _ctx: &Json_table_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_table_clause}.
 * @param ctx the parse tree
 */
fn exit_json_table_clause(&mut self, _ctx: &Json_table_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_array_element}.
 * @param ctx the parse tree
 */
fn enter_json_array_element(&mut self, _ctx: &Json_array_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_array_element}.
 * @param ctx the parse tree
 */
fn exit_json_array_element(&mut self, _ctx: &Json_array_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_on_null_clause}.
 * @param ctx the parse tree
 */
fn enter_json_on_null_clause(&mut self, _ctx: &Json_on_null_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_on_null_clause}.
 * @param ctx the parse tree
 */
fn exit_json_on_null_clause(&mut self, _ctx: &Json_on_null_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_return_clause}.
 * @param ctx the parse tree
 */
fn enter_json_return_clause(&mut self, _ctx: &Json_return_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_return_clause}.
 * @param ctx the parse tree
 */
fn exit_json_return_clause(&mut self, _ctx: &Json_return_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_transform_op}.
 * @param ctx the parse tree
 */
fn enter_json_transform_op(&mut self, _ctx: &Json_transform_opContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_transform_op}.
 * @param ctx the parse tree
 */
fn exit_json_transform_op(&mut self, _ctx: &Json_transform_opContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_column_clause}.
 * @param ctx the parse tree
 */
fn enter_json_column_clause(&mut self, _ctx: &Json_column_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_column_clause}.
 * @param ctx the parse tree
 */
fn exit_json_column_clause(&mut self, _ctx: &Json_column_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_column_definition}.
 * @param ctx the parse tree
 */
fn enter_json_column_definition(&mut self, _ctx: &Json_column_definitionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_column_definition}.
 * @param ctx the parse tree
 */
fn exit_json_column_definition(&mut self, _ctx: &Json_column_definitionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_query_returning_clause}.
 * @param ctx the parse tree
 */
fn enter_json_query_returning_clause(&mut self, _ctx: &Json_query_returning_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_query_returning_clause}.
 * @param ctx the parse tree
 */
fn exit_json_query_returning_clause(&mut self, _ctx: &Json_query_returning_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_query_return_type}.
 * @param ctx the parse tree
 */
fn enter_json_query_return_type(&mut self, _ctx: &Json_query_return_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_query_return_type}.
 * @param ctx the parse tree
 */
fn exit_json_query_return_type(&mut self, _ctx: &Json_query_return_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_query_wrapper_clause}.
 * @param ctx the parse tree
 */
fn enter_json_query_wrapper_clause(&mut self, _ctx: &Json_query_wrapper_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_query_wrapper_clause}.
 * @param ctx the parse tree
 */
fn exit_json_query_wrapper_clause(&mut self, _ctx: &Json_query_wrapper_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_query_on_error_clause}.
 * @param ctx the parse tree
 */
fn enter_json_query_on_error_clause(&mut self, _ctx: &Json_query_on_error_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_query_on_error_clause}.
 * @param ctx the parse tree
 */
fn exit_json_query_on_error_clause(&mut self, _ctx: &Json_query_on_error_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_query_on_empty_clause}.
 * @param ctx the parse tree
 */
fn enter_json_query_on_empty_clause(&mut self, _ctx: &Json_query_on_empty_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_query_on_empty_clause}.
 * @param ctx the parse tree
 */
fn exit_json_query_on_empty_clause(&mut self, _ctx: &Json_query_on_empty_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_value_return_clause}.
 * @param ctx the parse tree
 */
fn enter_json_value_return_clause(&mut self, _ctx: &Json_value_return_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_value_return_clause}.
 * @param ctx the parse tree
 */
fn exit_json_value_return_clause(&mut self, _ctx: &Json_value_return_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_value_return_type}.
 * @param ctx the parse tree
 */
fn enter_json_value_return_type(&mut self, _ctx: &Json_value_return_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_value_return_type}.
 * @param ctx the parse tree
 */
fn exit_json_value_return_type(&mut self, _ctx: &Json_value_return_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#json_value_on_mismatch_clause}.
 * @param ctx the parse tree
 */
fn enter_json_value_on_mismatch_clause(&mut self, _ctx: &Json_value_on_mismatch_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#json_value_on_mismatch_clause}.
 * @param ctx the parse tree
 */
fn exit_json_value_on_mismatch_clause(&mut self, _ctx: &Json_value_on_mismatch_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#literal}.
 * @param ctx the parse tree
 */
fn enter_literal(&mut self, _ctx: &LiteralContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#literal}.
 * @param ctx the parse tree
 */
fn exit_literal(&mut self, _ctx: &LiteralContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#numeric_function_wrapper}.
 * @param ctx the parse tree
 */
fn enter_numeric_function_wrapper(&mut self, _ctx: &Numeric_function_wrapperContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#numeric_function_wrapper}.
 * @param ctx the parse tree
 */
fn exit_numeric_function_wrapper(&mut self, _ctx: &Numeric_function_wrapperContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#numeric_function}.
 * @param ctx the parse tree
 */
fn enter_numeric_function(&mut self, _ctx: &Numeric_functionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#numeric_function}.
 * @param ctx the parse tree
 */
fn exit_numeric_function(&mut self, _ctx: &Numeric_functionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#listagg_overflow_clause}.
 * @param ctx the parse tree
 */
fn enter_listagg_overflow_clause(&mut self, _ctx: &Listagg_overflow_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#listagg_overflow_clause}.
 * @param ctx the parse tree
 */
fn exit_listagg_overflow_clause(&mut self, _ctx: &Listagg_overflow_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#other_function}.
 * @param ctx the parse tree
 */
fn enter_other_function(&mut self, _ctx: &Other_functionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#other_function}.
 * @param ctx the parse tree
 */
fn exit_other_function(&mut self, _ctx: &Other_functionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#over_clause_keyword}.
 * @param ctx the parse tree
 */
fn enter_over_clause_keyword(&mut self, _ctx: &Over_clause_keywordContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#over_clause_keyword}.
 * @param ctx the parse tree
 */
fn exit_over_clause_keyword(&mut self, _ctx: &Over_clause_keywordContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#within_or_over_clause_keyword}.
 * @param ctx the parse tree
 */
fn enter_within_or_over_clause_keyword(&mut self, _ctx: &Within_or_over_clause_keywordContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#within_or_over_clause_keyword}.
 * @param ctx the parse tree
 */
fn exit_within_or_over_clause_keyword(&mut self, _ctx: &Within_or_over_clause_keywordContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#standard_prediction_function_keyword}.
 * @param ctx the parse tree
 */
fn enter_standard_prediction_function_keyword(&mut self, _ctx: &Standard_prediction_function_keywordContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#standard_prediction_function_keyword}.
 * @param ctx the parse tree
 */
fn exit_standard_prediction_function_keyword(&mut self, _ctx: &Standard_prediction_function_keywordContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#over_clause}.
 * @param ctx the parse tree
 */
fn enter_over_clause(&mut self, _ctx: &Over_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#over_clause}.
 * @param ctx the parse tree
 */
fn exit_over_clause(&mut self, _ctx: &Over_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#windowing_clause}.
 * @param ctx the parse tree
 */
fn enter_windowing_clause(&mut self, _ctx: &Windowing_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#windowing_clause}.
 * @param ctx the parse tree
 */
fn exit_windowing_clause(&mut self, _ctx: &Windowing_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#windowing_type}.
 * @param ctx the parse tree
 */
fn enter_windowing_type(&mut self, _ctx: &Windowing_typeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#windowing_type}.
 * @param ctx the parse tree
 */
fn exit_windowing_type(&mut self, _ctx: &Windowing_typeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#windowing_elements}.
 * @param ctx the parse tree
 */
fn enter_windowing_elements(&mut self, _ctx: &Windowing_elementsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#windowing_elements}.
 * @param ctx the parse tree
 */
fn exit_windowing_elements(&mut self, _ctx: &Windowing_elementsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#using_clause}.
 * @param ctx the parse tree
 */
fn enter_using_clause(&mut self, _ctx: &Using_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#using_clause}.
 * @param ctx the parse tree
 */
fn exit_using_clause(&mut self, _ctx: &Using_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#using_element}.
 * @param ctx the parse tree
 */
fn enter_using_element(&mut self, _ctx: &Using_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#using_element}.
 * @param ctx the parse tree
 */
fn exit_using_element(&mut self, _ctx: &Using_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#assignable_element}.
 * @param ctx the parse tree
 */
fn enter_assignable_element(&mut self, _ctx: &Assignable_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#assignable_element}.
 * @param ctx the parse tree
 */
fn exit_assignable_element(&mut self, _ctx: &Assignable_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#collect_order_by_part}.
 * @param ctx the parse tree
 */
fn enter_collect_order_by_part(&mut self, _ctx: &Collect_order_by_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#collect_order_by_part}.
 * @param ctx the parse tree
 */
fn exit_collect_order_by_part(&mut self, _ctx: &Collect_order_by_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#within_or_over_part}.
 * @param ctx the parse tree
 */
fn enter_within_or_over_part(&mut self, _ctx: &Within_or_over_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#within_or_over_part}.
 * @param ctx the parse tree
 */
fn exit_within_or_over_part(&mut self, _ctx: &Within_or_over_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#string_delimiter}.
 * @param ctx the parse tree
 */
fn enter_string_delimiter(&mut self, _ctx: &String_delimiterContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#string_delimiter}.
 * @param ctx the parse tree
 */
fn exit_string_delimiter(&mut self, _ctx: &String_delimiterContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cost_matrix_clause}.
 * @param ctx the parse tree
 */
fn enter_cost_matrix_clause(&mut self, _ctx: &Cost_matrix_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cost_matrix_clause}.
 * @param ctx the parse tree
 */
fn exit_cost_matrix_clause(&mut self, _ctx: &Cost_matrix_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_passing_clause}.
 * @param ctx the parse tree
 */
fn enter_xml_passing_clause(&mut self, _ctx: &Xml_passing_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_passing_clause}.
 * @param ctx the parse tree
 */
fn exit_xml_passing_clause(&mut self, _ctx: &Xml_passing_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_attributes_clause}.
 * @param ctx the parse tree
 */
fn enter_xml_attributes_clause(&mut self, _ctx: &Xml_attributes_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_attributes_clause}.
 * @param ctx the parse tree
 */
fn exit_xml_attributes_clause(&mut self, _ctx: &Xml_attributes_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_namespaces_clause}.
 * @param ctx the parse tree
 */
fn enter_xml_namespaces_clause(&mut self, _ctx: &Xml_namespaces_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_namespaces_clause}.
 * @param ctx the parse tree
 */
fn exit_xml_namespaces_clause(&mut self, _ctx: &Xml_namespaces_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_table_column}.
 * @param ctx the parse tree
 */
fn enter_xml_table_column(&mut self, _ctx: &Xml_table_columnContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_table_column}.
 * @param ctx the parse tree
 */
fn exit_xml_table_column(&mut self, _ctx: &Xml_table_columnContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_general_default_part}.
 * @param ctx the parse tree
 */
fn enter_xml_general_default_part(&mut self, _ctx: &Xml_general_default_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_general_default_part}.
 * @param ctx the parse tree
 */
fn exit_xml_general_default_part(&mut self, _ctx: &Xml_general_default_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_multiuse_expression_element}.
 * @param ctx the parse tree
 */
fn enter_xml_multiuse_expression_element(&mut self, _ctx: &Xml_multiuse_expression_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_multiuse_expression_element}.
 * @param ctx the parse tree
 */
fn exit_xml_multiuse_expression_element(&mut self, _ctx: &Xml_multiuse_expression_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmlroot_param_version_part}.
 * @param ctx the parse tree
 */
fn enter_xmlroot_param_version_part(&mut self, _ctx: &Xmlroot_param_version_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmlroot_param_version_part}.
 * @param ctx the parse tree
 */
fn exit_xmlroot_param_version_part(&mut self, _ctx: &Xmlroot_param_version_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmlroot_param_standalone_part}.
 * @param ctx the parse tree
 */
fn enter_xmlroot_param_standalone_part(&mut self, _ctx: &Xmlroot_param_standalone_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmlroot_param_standalone_part}.
 * @param ctx the parse tree
 */
fn exit_xmlroot_param_standalone_part(&mut self, _ctx: &Xmlroot_param_standalone_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmlserialize_param_enconding_part}.
 * @param ctx the parse tree
 */
fn enter_xmlserialize_param_enconding_part(&mut self, _ctx: &Xmlserialize_param_enconding_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmlserialize_param_enconding_part}.
 * @param ctx the parse tree
 */
fn exit_xmlserialize_param_enconding_part(&mut self, _ctx: &Xmlserialize_param_enconding_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmlserialize_param_version_part}.
 * @param ctx the parse tree
 */
fn enter_xmlserialize_param_version_part(&mut self, _ctx: &Xmlserialize_param_version_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmlserialize_param_version_part}.
 * @param ctx the parse tree
 */
fn exit_xmlserialize_param_version_part(&mut self, _ctx: &Xmlserialize_param_version_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmlserialize_param_ident_part}.
 * @param ctx the parse tree
 */
fn enter_xmlserialize_param_ident_part(&mut self, _ctx: &Xmlserialize_param_ident_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmlserialize_param_ident_part}.
 * @param ctx the parse tree
 */
fn exit_xmlserialize_param_ident_part(&mut self, _ctx: &Xmlserialize_param_ident_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#annotations_clause}.
 * @param ctx the parse tree
 */
fn enter_annotations_clause(&mut self, _ctx: &Annotations_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#annotations_clause}.
 * @param ctx the parse tree
 */
fn exit_annotations_clause(&mut self, _ctx: &Annotations_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#annotations_list}.
 * @param ctx the parse tree
 */
fn enter_annotations_list(&mut self, _ctx: &Annotations_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#annotations_list}.
 * @param ctx the parse tree
 */
fn exit_annotations_list(&mut self, _ctx: &Annotations_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#annotation}.
 * @param ctx the parse tree
 */
fn enter_annotation(&mut self, _ctx: &AnnotationContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#annotation}.
 * @param ctx the parse tree
 */
fn exit_annotation(&mut self, _ctx: &AnnotationContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sql_plus_command}.
 * @param ctx the parse tree
 */
fn enter_sql_plus_command(&mut self, _ctx: &Sql_plus_commandContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sql_plus_command}.
 * @param ctx the parse tree
 */
fn exit_sql_plus_command(&mut self, _ctx: &Sql_plus_commandContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#start_command}.
 * @param ctx the parse tree
 */
fn enter_start_command(&mut self, _ctx: &Start_commandContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#start_command}.
 * @param ctx the parse tree
 */
fn exit_start_command(&mut self, _ctx: &Start_commandContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#whenever_command}.
 * @param ctx the parse tree
 */
fn enter_whenever_command(&mut self, _ctx: &Whenever_commandContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#whenever_command}.
 * @param ctx the parse tree
 */
fn exit_whenever_command(&mut self, _ctx: &Whenever_commandContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#set_command}.
 * @param ctx the parse tree
 */
fn enter_set_command(&mut self, _ctx: &Set_commandContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#set_command}.
 * @param ctx the parse tree
 */
fn exit_set_command(&mut self, _ctx: &Set_commandContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#timing_command}.
 * @param ctx the parse tree
 */
fn enter_timing_command(&mut self, _ctx: &Timing_commandContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#timing_command}.
 * @param ctx the parse tree
 */
fn exit_timing_command(&mut self, _ctx: &Timing_commandContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#clear_command}.
 * @param ctx the parse tree
 */
fn enter_clear_command(&mut self, _ctx: &Clear_commandContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#clear_command}.
 * @param ctx the parse tree
 */
fn exit_clear_command(&mut self, _ctx: &Clear_commandContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#partition_extension_clause}.
 * @param ctx the parse tree
 */
fn enter_partition_extension_clause(&mut self, _ctx: &Partition_extension_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#partition_extension_clause}.
 * @param ctx the parse tree
 */
fn exit_partition_extension_clause(&mut self, _ctx: &Partition_extension_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_alias}.
 * @param ctx the parse tree
 */
fn enter_column_alias(&mut self, _ctx: &Column_aliasContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_alias}.
 * @param ctx the parse tree
 */
fn exit_column_alias(&mut self, _ctx: &Column_aliasContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_alias}.
 * @param ctx the parse tree
 */
fn enter_table_alias(&mut self, _ctx: &Table_aliasContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_alias}.
 * @param ctx the parse tree
 */
fn exit_table_alias(&mut self, _ctx: &Table_aliasContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#where_clause}.
 * @param ctx the parse tree
 */
fn enter_where_clause(&mut self, _ctx: &Where_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#where_clause}.
 * @param ctx the parse tree
 */
fn exit_where_clause(&mut self, _ctx: &Where_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#into_clause}.
 * @param ctx the parse tree
 */
fn enter_into_clause(&mut self, _ctx: &Into_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#into_clause}.
 * @param ctx the parse tree
 */
fn exit_into_clause(&mut self, _ctx: &Into_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xml_column_name}.
 * @param ctx the parse tree
 */
fn enter_xml_column_name(&mut self, _ctx: &Xml_column_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xml_column_name}.
 * @param ctx the parse tree
 */
fn exit_xml_column_name(&mut self, _ctx: &Xml_column_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cost_class_name}.
 * @param ctx the parse tree
 */
fn enter_cost_class_name(&mut self, _ctx: &Cost_class_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cost_class_name}.
 * @param ctx the parse tree
 */
fn exit_cost_class_name(&mut self, _ctx: &Cost_class_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#attribute_name}.
 * @param ctx the parse tree
 */
fn enter_attribute_name(&mut self, _ctx: &Attribute_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#attribute_name}.
 * @param ctx the parse tree
 */
fn exit_attribute_name(&mut self, _ctx: &Attribute_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#savepoint_name}.
 * @param ctx the parse tree
 */
fn enter_savepoint_name(&mut self, _ctx: &Savepoint_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#savepoint_name}.
 * @param ctx the parse tree
 */
fn exit_savepoint_name(&mut self, _ctx: &Savepoint_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#rollback_segment_name}.
 * @param ctx the parse tree
 */
fn enter_rollback_segment_name(&mut self, _ctx: &Rollback_segment_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#rollback_segment_name}.
 * @param ctx the parse tree
 */
fn exit_rollback_segment_name(&mut self, _ctx: &Rollback_segment_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#schema_name}.
 * @param ctx the parse tree
 */
fn enter_schema_name(&mut self, _ctx: &Schema_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#schema_name}.
 * @param ctx the parse tree
 */
fn exit_schema_name(&mut self, _ctx: &Schema_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#routine_name}.
 * @param ctx the parse tree
 */
fn enter_routine_name(&mut self, _ctx: &Routine_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#routine_name}.
 * @param ctx the parse tree
 */
fn exit_routine_name(&mut self, _ctx: &Routine_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#package_name}.
 * @param ctx the parse tree
 */
fn enter_package_name(&mut self, _ctx: &Package_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#package_name}.
 * @param ctx the parse tree
 */
fn exit_package_name(&mut self, _ctx: &Package_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#implementation_type_name}.
 * @param ctx the parse tree
 */
fn enter_implementation_type_name(&mut self, _ctx: &Implementation_type_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#implementation_type_name}.
 * @param ctx the parse tree
 */
fn exit_implementation_type_name(&mut self, _ctx: &Implementation_type_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#parameter_name}.
 * @param ctx the parse tree
 */
fn enter_parameter_name(&mut self, _ctx: &Parameter_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#parameter_name}.
 * @param ctx the parse tree
 */
fn exit_parameter_name(&mut self, _ctx: &Parameter_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#reference_model_name}.
 * @param ctx the parse tree
 */
fn enter_reference_model_name(&mut self, _ctx: &Reference_model_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#reference_model_name}.
 * @param ctx the parse tree
 */
fn exit_reference_model_name(&mut self, _ctx: &Reference_model_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#main_model_name}.
 * @param ctx the parse tree
 */
fn enter_main_model_name(&mut self, _ctx: &Main_model_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#main_model_name}.
 * @param ctx the parse tree
 */
fn exit_main_model_name(&mut self, _ctx: &Main_model_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#container_tableview_name}.
 * @param ctx the parse tree
 */
fn enter_container_tableview_name(&mut self, _ctx: &Container_tableview_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#container_tableview_name}.
 * @param ctx the parse tree
 */
fn exit_container_tableview_name(&mut self, _ctx: &Container_tableview_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#aggregate_function_name}.
 * @param ctx the parse tree
 */
fn enter_aggregate_function_name(&mut self, _ctx: &Aggregate_function_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#aggregate_function_name}.
 * @param ctx the parse tree
 */
fn exit_aggregate_function_name(&mut self, _ctx: &Aggregate_function_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#query_name}.
 * @param ctx the parse tree
 */
fn enter_query_name(&mut self, _ctx: &Query_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#query_name}.
 * @param ctx the parse tree
 */
fn exit_query_name(&mut self, _ctx: &Query_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#grantee_name}.
 * @param ctx the parse tree
 */
fn enter_grantee_name(&mut self, _ctx: &Grantee_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#grantee_name}.
 * @param ctx the parse tree
 */
fn exit_grantee_name(&mut self, _ctx: &Grantee_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#role_name}.
 * @param ctx the parse tree
 */
fn enter_role_name(&mut self, _ctx: &Role_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#role_name}.
 * @param ctx the parse tree
 */
fn exit_role_name(&mut self, _ctx: &Role_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#constraint_name}.
 * @param ctx the parse tree
 */
fn enter_constraint_name(&mut self, _ctx: &Constraint_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#constraint_name}.
 * @param ctx the parse tree
 */
fn exit_constraint_name(&mut self, _ctx: &Constraint_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#label_name}.
 * @param ctx the parse tree
 */
fn enter_label_name(&mut self, _ctx: &Label_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#label_name}.
 * @param ctx the parse tree
 */
fn exit_label_name(&mut self, _ctx: &Label_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_name}.
 * @param ctx the parse tree
 */
fn enter_type_name(&mut self, _ctx: &Type_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_name}.
 * @param ctx the parse tree
 */
fn exit_type_name(&mut self, _ctx: &Type_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#sequence_name}.
 * @param ctx the parse tree
 */
fn enter_sequence_name(&mut self, _ctx: &Sequence_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#sequence_name}.
 * @param ctx the parse tree
 */
fn exit_sequence_name(&mut self, _ctx: &Sequence_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#exception_name}.
 * @param ctx the parse tree
 */
fn enter_exception_name(&mut self, _ctx: &Exception_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#exception_name}.
 * @param ctx the parse tree
 */
fn exit_exception_name(&mut self, _ctx: &Exception_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#function_name}.
 * @param ctx the parse tree
 */
fn enter_function_name(&mut self, _ctx: &Function_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#function_name}.
 * @param ctx the parse tree
 */
fn exit_function_name(&mut self, _ctx: &Function_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#procedure_name}.
 * @param ctx the parse tree
 */
fn enter_procedure_name(&mut self, _ctx: &Procedure_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#procedure_name}.
 * @param ctx the parse tree
 */
fn exit_procedure_name(&mut self, _ctx: &Procedure_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#trigger_name}.
 * @param ctx the parse tree
 */
fn enter_trigger_name(&mut self, _ctx: &Trigger_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#trigger_name}.
 * @param ctx the parse tree
 */
fn exit_trigger_name(&mut self, _ctx: &Trigger_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#variable_name}.
 * @param ctx the parse tree
 */
fn enter_variable_name(&mut self, _ctx: &Variable_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#variable_name}.
 * @param ctx the parse tree
 */
fn exit_variable_name(&mut self, _ctx: &Variable_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#index_name}.
 * @param ctx the parse tree
 */
fn enter_index_name(&mut self, _ctx: &Index_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#index_name}.
 * @param ctx the parse tree
 */
fn exit_index_name(&mut self, _ctx: &Index_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#cursor_name}.
 * @param ctx the parse tree
 */
fn enter_cursor_name(&mut self, _ctx: &Cursor_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#cursor_name}.
 * @param ctx the parse tree
 */
fn exit_cursor_name(&mut self, _ctx: &Cursor_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#record_name}.
 * @param ctx the parse tree
 */
fn enter_record_name(&mut self, _ctx: &Record_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#record_name}.
 * @param ctx the parse tree
 */
fn exit_record_name(&mut self, _ctx: &Record_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#link_name}.
 * @param ctx the parse tree
 */
fn enter_link_name(&mut self, _ctx: &Link_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#link_name}.
 * @param ctx the parse tree
 */
fn exit_link_name(&mut self, _ctx: &Link_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#local_link_name}.
 * @param ctx the parse tree
 */
fn enter_local_link_name(&mut self, _ctx: &Local_link_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#local_link_name}.
 * @param ctx the parse tree
 */
fn exit_local_link_name(&mut self, _ctx: &Local_link_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#connection_qualifier}.
 * @param ctx the parse tree
 */
fn enter_connection_qualifier(&mut self, _ctx: &Connection_qualifierContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#connection_qualifier}.
 * @param ctx the parse tree
 */
fn exit_connection_qualifier(&mut self, _ctx: &Connection_qualifierContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_name}.
 * @param ctx the parse tree
 */
fn enter_column_name(&mut self, _ctx: &Column_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_name}.
 * @param ctx the parse tree
 */
fn exit_column_name(&mut self, _ctx: &Column_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#tableview_name}.
 * @param ctx the parse tree
 */
fn enter_tableview_name(&mut self, _ctx: &Tableview_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#tableview_name}.
 * @param ctx the parse tree
 */
fn exit_tableview_name(&mut self, _ctx: &Tableview_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#xmltable}.
 * @param ctx the parse tree
 */
fn enter_xmltable(&mut self, _ctx: &XmltableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#xmltable}.
 * @param ctx the parse tree
 */
fn exit_xmltable(&mut self, _ctx: &XmltableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#char_set_name}.
 * @param ctx the parse tree
 */
fn enter_char_set_name(&mut self, _ctx: &Char_set_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#char_set_name}.
 * @param ctx the parse tree
 */
fn exit_char_set_name(&mut self, _ctx: &Char_set_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#synonym_name}.
 * @param ctx the parse tree
 */
fn enter_synonym_name(&mut self, _ctx: &Synonym_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#synonym_name}.
 * @param ctx the parse tree
 */
fn exit_synonym_name(&mut self, _ctx: &Synonym_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#schema_object_name}.
 * @param ctx the parse tree
 */
fn enter_schema_object_name(&mut self, _ctx: &Schema_object_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#schema_object_name}.
 * @param ctx the parse tree
 */
fn exit_schema_object_name(&mut self, _ctx: &Schema_object_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#dir_object_name}.
 * @param ctx the parse tree
 */
fn enter_dir_object_name(&mut self, _ctx: &Dir_object_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#dir_object_name}.
 * @param ctx the parse tree
 */
fn exit_dir_object_name(&mut self, _ctx: &Dir_object_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#user_object_name}.
 * @param ctx the parse tree
 */
fn enter_user_object_name(&mut self, _ctx: &User_object_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#user_object_name}.
 * @param ctx the parse tree
 */
fn exit_user_object_name(&mut self, _ctx: &User_object_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#grant_object_name}.
 * @param ctx the parse tree
 */
fn enter_grant_object_name(&mut self, _ctx: &Grant_object_nameContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#grant_object_name}.
 * @param ctx the parse tree
 */
fn exit_grant_object_name(&mut self, _ctx: &Grant_object_nameContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#column_list}.
 * @param ctx the parse tree
 */
fn enter_column_list(&mut self, _ctx: &Column_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#column_list}.
 * @param ctx the parse tree
 */
fn exit_column_list(&mut self, _ctx: &Column_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#paren_column_list}.
 * @param ctx the parse tree
 */
fn enter_paren_column_list(&mut self, _ctx: &Paren_column_listContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#paren_column_list}.
 * @param ctx the parse tree
 */
fn exit_paren_column_list(&mut self, _ctx: &Paren_column_listContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#keep_clause}.
 * @param ctx the parse tree
 */
fn enter_keep_clause(&mut self, _ctx: &Keep_clauseContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#keep_clause}.
 * @param ctx the parse tree
 */
fn exit_keep_clause(&mut self, _ctx: &Keep_clauseContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#function_argument}.
 * @param ctx the parse tree
 */
fn enter_function_argument(&mut self, _ctx: &Function_argumentContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#function_argument}.
 * @param ctx the parse tree
 */
fn exit_function_argument(&mut self, _ctx: &Function_argumentContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#function_argument_analytic}.
 * @param ctx the parse tree
 */
fn enter_function_argument_analytic(&mut self, _ctx: &Function_argument_analyticContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#function_argument_analytic}.
 * @param ctx the parse tree
 */
fn exit_function_argument_analytic(&mut self, _ctx: &Function_argument_analyticContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#function_argument_modeling}.
 * @param ctx the parse tree
 */
fn enter_function_argument_modeling(&mut self, _ctx: &Function_argument_modelingContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#function_argument_modeling}.
 * @param ctx the parse tree
 */
fn exit_function_argument_modeling(&mut self, _ctx: &Function_argument_modelingContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#respect_or_ignore_nulls}.
 * @param ctx the parse tree
 */
fn enter_respect_or_ignore_nulls(&mut self, _ctx: &Respect_or_ignore_nullsContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#respect_or_ignore_nulls}.
 * @param ctx the parse tree
 */
fn exit_respect_or_ignore_nulls(&mut self, _ctx: &Respect_or_ignore_nullsContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#argument}.
 * @param ctx the parse tree
 */
fn enter_argument(&mut self, _ctx: &ArgumentContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#argument}.
 * @param ctx the parse tree
 */
fn exit_argument(&mut self, _ctx: &ArgumentContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#type_spec}.
 * @param ctx the parse tree
 */
fn enter_type_spec(&mut self, _ctx: &Type_specContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#type_spec}.
 * @param ctx the parse tree
 */
fn exit_type_spec(&mut self, _ctx: &Type_specContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#datatype}.
 * @param ctx the parse tree
 */
fn enter_datatype(&mut self, _ctx: &DatatypeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#datatype}.
 * @param ctx the parse tree
 */
fn exit_datatype(&mut self, _ctx: &DatatypeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#precision_part}.
 * @param ctx the parse tree
 */
fn enter_precision_part(&mut self, _ctx: &Precision_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#precision_part}.
 * @param ctx the parse tree
 */
fn exit_precision_part(&mut self, _ctx: &Precision_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#native_datatype_element}.
 * @param ctx the parse tree
 */
fn enter_native_datatype_element(&mut self, _ctx: &Native_datatype_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#native_datatype_element}.
 * @param ctx the parse tree
 */
fn exit_native_datatype_element(&mut self, _ctx: &Native_datatype_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#bind_variable}.
 * @param ctx the parse tree
 */
fn enter_bind_variable(&mut self, _ctx: &Bind_variableContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#bind_variable}.
 * @param ctx the parse tree
 */
fn exit_bind_variable(&mut self, _ctx: &Bind_variableContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#general_element}.
 * @param ctx the parse tree
 */
fn enter_general_element(&mut self, _ctx: &General_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#general_element}.
 * @param ctx the parse tree
 */
fn exit_general_element(&mut self, _ctx: &General_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#general_element_part}.
 * @param ctx the parse tree
 */
fn enter_general_element_part(&mut self, _ctx: &General_element_partContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#general_element_part}.
 * @param ctx the parse tree
 */
fn exit_general_element_part(&mut self, _ctx: &General_element_partContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#table_element}.
 * @param ctx the parse tree
 */
fn enter_table_element(&mut self, _ctx: &Table_elementContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#table_element}.
 * @param ctx the parse tree
 */
fn exit_table_element(&mut self, _ctx: &Table_elementContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#object_privilege}.
 * @param ctx the parse tree
 */
fn enter_object_privilege(&mut self, _ctx: &Object_privilegeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#object_privilege}.
 * @param ctx the parse tree
 */
fn exit_object_privilege(&mut self, _ctx: &Object_privilegeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#system_privilege}.
 * @param ctx the parse tree
 */
fn enter_system_privilege(&mut self, _ctx: &System_privilegeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#system_privilege}.
 * @param ctx the parse tree
 */
fn exit_system_privilege(&mut self, _ctx: &System_privilegeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#constant}.
 * @param ctx the parse tree
 */
fn enter_constant(&mut self, _ctx: &ConstantContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#constant}.
 * @param ctx the parse tree
 */
fn exit_constant(&mut self, _ctx: &ConstantContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#numeric}.
 * @param ctx the parse tree
 */
fn enter_numeric(&mut self, _ctx: &NumericContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#numeric}.
 * @param ctx the parse tree
 */
fn exit_numeric(&mut self, _ctx: &NumericContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#numeric_negative}.
 * @param ctx the parse tree
 */
fn enter_numeric_negative(&mut self, _ctx: &Numeric_negativeContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#numeric_negative}.
 * @param ctx the parse tree
 */
fn exit_numeric_negative(&mut self, _ctx: &Numeric_negativeContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#quoted_string}.
 * @param ctx the parse tree
 */
fn enter_quoted_string(&mut self, _ctx: &Quoted_stringContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#quoted_string}.
 * @param ctx the parse tree
 */
fn exit_quoted_string(&mut self, _ctx: &Quoted_stringContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#identifier}.
 * @param ctx the parse tree
 */
fn enter_identifier(&mut self, _ctx: &IdentifierContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#identifier}.
 * @param ctx the parse tree
 */
fn exit_identifier(&mut self, _ctx: &IdentifierContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#id_expression}.
 * @param ctx the parse tree
 */
fn enter_id_expression(&mut self, _ctx: &Id_expressionContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#id_expression}.
 * @param ctx the parse tree
 */
fn exit_id_expression(&mut self, _ctx: &Id_expressionContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#inquiry_directive}.
 * @param ctx the parse tree
 */
fn enter_inquiry_directive(&mut self, _ctx: &Inquiry_directiveContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#inquiry_directive}.
 * @param ctx the parse tree
 */
fn exit_inquiry_directive(&mut self, _ctx: &Inquiry_directiveContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#outer_join_sign}.
 * @param ctx the parse tree
 */
fn enter_outer_join_sign(&mut self, _ctx: &Outer_join_signContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#outer_join_sign}.
 * @param ctx the parse tree
 */
fn exit_outer_join_sign(&mut self, _ctx: &Outer_join_signContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#regular_id}.
 * @param ctx the parse tree
 */
fn enter_regular_id(&mut self, _ctx: &Regular_idContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#regular_id}.
 * @param ctx the parse tree
 */
fn exit_regular_id(&mut self, _ctx: &Regular_idContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#non_reserved_keywords_in_18c}.
 * @param ctx the parse tree
 */
fn enter_non_reserved_keywords_in_18c(&mut self, _ctx: &Non_reserved_keywords_in_18cContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#non_reserved_keywords_in_18c}.
 * @param ctx the parse tree
 */
fn exit_non_reserved_keywords_in_18c(&mut self, _ctx: &Non_reserved_keywords_in_18cContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#non_reserved_keywords_in_12c}.
 * @param ctx the parse tree
 */
fn enter_non_reserved_keywords_in_12c(&mut self, _ctx: &Non_reserved_keywords_in_12cContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#non_reserved_keywords_in_12c}.
 * @param ctx the parse tree
 */
fn exit_non_reserved_keywords_in_12c(&mut self, _ctx: &Non_reserved_keywords_in_12cContext<'input>) { }
/**
 * Enter a parse tree produced by {@link PlSqlParser#non_reserved_keywords_pre12c}.
 * @param ctx the parse tree
 */
fn enter_non_reserved_keywords_pre12c(&mut self, _ctx: &Non_reserved_keywords_pre12cContext<'input>) { }
/**
 * Exit a parse tree produced by {@link PlSqlParser#non_reserved_keywords_pre12c}.
 * @param ctx the parse tree
 */
fn exit_non_reserved_keywords_pre12c(&mut self, _ctx: &Non_reserved_keywords_pre12cContext<'input>) { }

}

antlr_rust::coerce_from!{ 'input : PlSqlParserListener<'input> }
