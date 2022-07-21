export const CW20_Demurrage = (client, fees) => {
    const use = (contractAddress) => {
        
        //done
        const balance = async (address) => {
            const result = await client.queryContractSmart(contractAddress, {balance: { Query }})
            return result.balance
        }
  
        const allowance = async (owner, spender) => {
            return client.queryContractSmart(contractAddress, {allowance: { owner, spender }})
        }
    
        const allAllowances = async (owner, startAfter, limit) => {
            return client.queryContractSmart(contractAddress, {all_allowances: { owner, start_after: startAfter, limit }})
        }
    
        const allAccounts = async (startAfter, limit) => {
            const accounts= await client.queryContractSmart(contractAddress, {all_accounts: { start_after: startAfter, limit }})
            return accounts.accounts
        }
    
        const tokenInfo = async () => {
            return client.queryContractSmart(contractAddress, {token_info: { }})
        }
    
        const minter = async () => {
            return client.queryContractSmart(contractAddress, {minter: { }})
        }

        const demurrageAmount = async () => {
            return client.queryContractSmart(contractAddress, {demurrage_amount: { }})
        }

        const taxLevel = async () => {
            return client.queryContractSmart(contractAddress, {tax_level: { }})
        }

        const sinkAddress = async () => {
            return client.queryContractSmart(contractAddress, {sink_account: { }})
        }
    
        // mints tokens, returns transactionHash
        const mint = async (senderAddress, recipient, amount) => {
            const result = await client.execute(senderAddress, contractAddress, {mint: {recipient, amount}}, fees.exec)
            return result.transactionHash
        }
    
        // transfers tokens, returns transactionHash
        const transfer = async (senderAddress, recipient, amount) => {
            const result = await client.execute(senderAddress, contractAddress, {transfer: {recipient, amount}}, fees.exec)
            return result.transactionHash
        }
    
        // burns tokens, returns transactionHash
        const burn = async (senderAddress, amount) => {
            const result = await client.execute(senderAddress, contractAddress, {burn: {amount}}, fees.exec)
            return result.transactionHash
        }
    
        const increaseAllowance = async (senderAddress, spender, amount) => {
            const result = await client.execute(senderAddress, contractAddress, {increase_allowance: {spender, amount}}, fees.exec)
            return result.transactionHash
        }
    
        const decreaseAllowance = async (senderAddress, spender, amount) => {
            const result = await client.execute(senderAddress, contractAddress, {decrease_allowance: {spender, amount}}, fees.exec)
            return result.transactionHash
        }
    
        const transferFrom = async (senderAddress, owner, recipient, amount) => {
            const result = await client.execute(senderAddress, contractAddress, {transfer_from: {owner, recipient, amount}}, fees.exec)
            return result.transactionHash
        }
    
        const jsonToBinary  = (json) => { return toBase64(toUtf8(JSON.stringify(json)))}
    
        const send = async (senderAddress, recipient, amount, msg) => {
            const result = await client.execute(senderAddress, contractAddress, {send: {recipient, amount, msg: jsonToBinary(msg)}}, fees.exec)
            return result.transactionHash
        }
    
        const sendFrom = async (senderAddress, owner, recipient, amount, msg) => {
            const result = await client.execute(senderAddress, contractAddress, {send_from: {owner, recipient, amount, msg: jsonToBinary(msg)}}, fees.exec)
            return result.transactionHash
        }

        const changeSinkAdress = async (address) => {
            const result = await client.execute(senderAddress, contractAddress, {change_sink_address: {address}}, fees.exec)
            return result.transactionHash
        }

        const changeTaxLevel = async (amount) => {
            const result = await client.execute(senderAddress, contractAddress, {change_tax_level: {amount}}, fees.exec)
            return result.transactionHash
        }
    
        return {
            contractAddress,
            balance,
            allowance,
            allAllowances,
            allAccounts,
            tokenInfo,
            minter,
            mint,
            transfer,
            burn,
            increaseAllowance,
            decreaseAllowance,
            transferFrom,
            send,
            sendFrom, 
            changeSinkAdress, 
            changeTaxLevel, 
            demurrageAmount, 
            taxLevel, 
            sinkAddress,
        }
    }
}