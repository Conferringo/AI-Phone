'use client';
import {createContext,useContext} from 'react';
export type DeviceDefaults={phone:{cpus:number;memoryMb:number;storageGb:number};desktop:{cpus:number;memoryMb:number}};
export const creationDefaults:DeviceDefaults={phone:{cpus:4,memoryMb:3072,storageGb:16},desktop:{cpus:2,memoryMb:2560}};
const Context=createContext<DeviceDefaults>(creationDefaults);
export function DeviceDefaultsProvider({value,children}:{value:DeviceDefaults;children:React.ReactNode}){return <Context.Provider value={value}>{children}</Context.Provider>;}
export const useDeviceDefaults=()=>useContext(Context);
