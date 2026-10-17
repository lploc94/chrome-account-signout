import {invoke} from '@tauri-apps/api/core';
export interface Account {id:string;name:string;email:string;is_primary:boolean}
export interface Profile {id:string;name:string;accounts:Account[];error:string|null}
export interface Pending {token:string;email:string}
export interface Status {id:string;phase:string;message:string}
export interface Api {list():Promise<Profile[]>;prepare(id:string):Promise<Pending>;confirm(token:string):Promise<string>;cancel(id:string):Promise<void>;status(id:string):Promise<Status>}
export const ipc:Api={list:()=>invoke('list_accounts'),prepare:id=>invoke('prepare_logout',{id}),confirm:token=>invoke('confirm_logout',{token}),cancel:id=>invoke('cancel_logout',{id}),status:id=>invoke('operation_status',{id})};
