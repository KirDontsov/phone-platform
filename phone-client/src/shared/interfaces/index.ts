export type Role='owner'|'manager'|'reviewer'
export interface Organization {id:string;name:string;timezone:string;callback_minutes:number;retention_days:number;working_hours?:unknown;callback_allow_inbound?:boolean;callback_min_duration?:number}
export interface Me {user:{id:string;name:string;email:string};organization:Organization;organizations:{id:string;name:string;role:Role}[];role:Role;csrf_token:string;demo_mode:boolean}
export type Filter = {op:'and'|'or';rules:Filter[]} | {op:'not';rule:Filter} | {field:string;op:string;value?:unknown}
export interface Taxon {id:string;name:string;color:string;archived:boolean}
export interface Taxonomy {tags:Taxon[];statuses:Taxon[]}
export interface Call {id:string;provider_id:string;started_at:string;phone:string;contact_name:string;employee:string;direction:string;provider_result:string;human_connected:boolean;duration:number;stage:string;status_id:string|null;tag_ids:string[];contact_kind:string;score:number|null;has_recording:boolean;note:string;manual_lock:boolean;version:number;coverage_complete:boolean}
export interface Coverage {complete:boolean;reason:string}
export interface CallList {items:Call[];total:number;next_cursor:string|null;coverage:Coverage}
export interface Segment {id:string;speaker:string;role:string;start:number;end:number;text:string}
export interface Transcript {id:string;version:number;provider:string;state:string;language:string;role_confidence:string;segments:Segment[];created_at:string}
export interface Evidence {segment_id:string;start:number;end:number;quote:string}
export interface Analysis {id:string;state:string;model:string;provider:string;prompt_version:string;rubric_version:string|number;transcript_id:string;summary:string;customer_intent:string;agreements:string[];next_steps:string[];suggested_status_id:string|null;suggested_tag_ids:string[];criteria:{id:string;name:string;weight:number;score:number|null;status:string;evidence:Evidence[];reason:string}[];score:number|null;abstention_reason:string|null;created_at:string;usage:{input_tokens:number;output_tokens:number;cost:number|null}}
export interface Recording {id:string;state?:string;mime_type?:string;duration?:number;size?:number;[key:string]:unknown}
export interface CallDetail {call:Call;legs:Record<string,unknown>[];recordings:Recording[];transcripts:Transcript[];analyses:Analysis[];history:Call[];audit:Record<string,unknown>[];external_links:Record<string,unknown>[]}
export interface Dashboard {totals:{calls:number;answered:number;missed:number;unhandled:number;overdue:number;analyzed:number;average_score:number|null};daily:{date:string;calls:number;missed:number}[];employees:{name:string;calls:number;average_score:number|null}[];scores:{bucket:string;count:number}[];coverage:Coverage}
export interface Callback {call_id:string;phone:string;employee:string;opened_at:string;due_at:string;attempts:number;state:string;reason:string}
export interface Connection {id:string;kind:string;state:string;config:Record<string,unknown>;last_error:string|null;last_sync_at:string|null}
export interface Rubric {id:string;name:string;version:number;rubric_version?:number;active:boolean;criteria:{id:string;name:string;weight:number;required:boolean;instruction:string}[]}
export interface SavedFilter {id:string;name:string;ast:Filter}
export interface Member {id:string;email:string;name:string;role:Role;extension:string|null}
export interface Invitation {id:string;email:string;role:Role;expires_at:string;revoked:boolean}
export interface Operations {jobs:Record<string,unknown>[];sync_runs:Record<string,unknown>[];deliveries:Record<string,unknown>[];usage:Record<string,unknown>;health:Record<string,unknown>;audit:Record<string,unknown>[]}
